// Build the installer without the build machine in it. OPSEC!!
//
//   pnpm app:build            (extra arguments go to `tauri build`)
//
// Rust keeps the source path of every dependency in the binary for its panic
// messages - `C:\Users\<name>\.cargo\registry\...` - which names the account
// that built it. This remaps those paths to neutral ones for the build, taken
// from this machine at build time so no path is written into the repository.
// After building it checks the binary and fails if the home folder is still in it.

import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const home = os.homedir()
const cargoHome = process.env.CARGO_HOME || path.join(home, '.cargo')
const rustupHome = process.env.RUSTUP_HOME || path.join(home, '.rustup')

const remaps = [
  [cargoHome, '/cargo'],
  [rustupHome, '/rustup'],
  [root, '/kryoto-desktop'],
  [home, '/home'],
]
const flags = remaps.map(([from, to]) => `--remap-path-prefix=${from}=${to}`).join(' ')
const env = { ...process.env, RUSTFLAGS: [process.env.RUSTFLAGS, flags].filter(Boolean).join(' ') }

// C code built from source (SDL2, through sdl2-sys) keeps __FILE__ paths in
// its asserts, and RUSTFLAGS never reach a C compiler. The C equivalent of the
// remap: MSVC trims a prefix off __FILE__, GCC and Clang map it.
const msvc = /msvc/.test(spawnSync('rustc', ['-vV'], { encoding: 'utf8' }).stdout || '')
const cflags = msvc
  ? remaps.map(([from]) => `/d1trimfile:${from}${path.sep}`).join(' ')
  : remaps.map(([from, to]) => `-ffile-prefix-map=${from}=${to}`).join(' ')
env.CFLAGS = [process.env.CFLAGS, cflags].filter(Boolean).join(' ')

// On Windows the build must not see Git's MSYS tools: in a bash step they
// sit ahead of everything on PATH, so OpenSSL's Configure finds MSYS perl
// (missing modules it needs) and the linker resolves to MSYS `link` instead
// of MSVC's. Drop those directories and put the real toolchain first. This
// runs for local builds too, where the same shadowing bites Git Bash users.
if (process.platform === 'win32') {
  const sep = ';'
  const parts = (env.PATH || '').split(sep)
  const shadow = /(^|[\\/])git[\\/](usr|mingw64)[\\/]bin[\\/]?$/i
  const kept = parts.filter((p) => !shadow.test(p.replace(/\//g, '\\')))
  const first = []
  const linker = env.CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER
  if (linker && existsSync(linker)) first.push(path.dirname(linker))
  for (const dir of ['C:\\Strawberry\\perl\\bin', 'C:\\Program Files\\NASM']) {
    if (existsSync(dir)) first.push(dir)
  }
  const seen = new Set()
  env.PATH = [...first, ...kept].filter((p) => p && !seen.has(p.toLowerCase()) && (seen.add(p.toLowerCase()), true)).join(sep)
}

const run = spawnSync(['pnpm tauri build', ...process.argv.slice(2)].join(' '), { cwd: root, env, stdio: 'inherit', shell: true })
if (run.status !== 0) process.exit(run.status ?? 1)

const exe = path.join(root, 'src-tauri', 'target', 'release', process.platform === 'win32' ? 'kryoto-desktop.exe' : 'kryoto-desktop')

// OpenSSL bakes its Configure --prefix (under Cargo's target/ dir) into its
// default enginesdir/modulesdir strings. The C compiler never sees RUSTFLAGS,
// so no --remap-path-prefix can reach them, and they name the build machine.
// They are dead defaults - the build is `no-module`, nothing is ever loaded
// from them, and the path does not exist on any player's PC either - so they
// are overwritten in place with a short neutral path, NUL-padded to the exact
// same byte length (they are C strings; the binary layout must not move).
// The leak check below then proves the binary is clean.
{
  const buf = readFileSync(exe)
  const text = buf.toString('latin1')
  const pattern = /([A-Za-z]:[\\/]|\/)[^\0"]*?target[\\/][^\\/"\0]+[\\/]build[\\/]openssl-sys-[^\\/"\0]+[\\/]out[\\/]openssl-build[\\/]install/g
  let patched = 0
  const clean = text.replace(pattern, (match) => {
    patched++
    const replacement = '/kryoto/openssl-install'
    return replacement + '\0'.repeat(match.length - replacement.length)
  })
  if (patched > 0) {
    writeFileSync(exe, Buffer.from(clean, 'latin1'))
    console.log(`\nNeutralized ${patched} baked OpenSSL install path(s).`)
  }
}

const bytes = readFileSync(exe).toString('latin1').toLowerCase()
const name = os.userInfo().username.toLowerCase()
// Each as the start of a path (followed by a separator), not as bare text: a
// home of `/root` (a container building as root) is also the tail of Steam's
// own `~/.steam/root`, which the app looks for on purpose.
const leaks = [home.toLowerCase(), `users\\${name}`, `users/${name}`, `home/${name}`]
  .flatMap((s) => [`${s}/`, `${s}\\`])
  .filter((s) => bytes.includes(s))
if (leaks.length) {
  console.error(`\nThe build still contains ${leaks.map((l) => JSON.stringify(l)).join(' and ')}. Do not ship it.`)
  process.exit(1)
}
console.log('\nNo build-machine paths in the binary.')
