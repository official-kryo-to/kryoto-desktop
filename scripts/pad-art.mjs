// Writes the controller drawings (src/lib/pad-art.ts) out as .svg files:
//   node --experimental-strip-types scripts/pad-art.mjs <out dir>
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { PAD_FAMILIES, padSvg } from '../src/lib/pad-art.ts'

const out = process.argv[2] ?? 'pad-art'
mkdirSync(out, { recursive: true })
for (const family of PAD_FAMILIES) {
  writeFileSync(join(out, `${family}.svg`), padSvg(family))
  console.log(join(out, `${family}.svg`))
}
