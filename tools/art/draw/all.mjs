// Перерисовывает картинки первого уровня платформера: node tools/art/draw/all.mjs [имя картинки…]
// Без имён — все. Пишет в games/platformer/images; картинка не шире листа атласа движка (2048 точек).
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";
import { IMAGES } from "./images.mjs";
import { CELL } from "./brush.mjs";

const ATLAS_SIDE = 2046;
const out = fileURLToPath(new URL("../../../games/platformer/images/", import.meta.url));
const only = process.argv.slice(2);

for (const [name, img] of Object.entries(IMAGES)) {
  if (only.length && !only.includes(name)) continue;
  let pipe = sharp(Buffer.from(img.draw().svg), { density: 72, limitInputPixels: false });
  if (img.crop) {
    pipe = sharp(await pipe.png().toBuffer()).extract({ left: img.crop[0], top: 0, width: img.crop[1], height: Math.round(img.cells[1] * CELL) });
  }
  if (img.ppc && img.ppc !== CELL) {
    pipe = sharp(await pipe.png().toBuffer()).resize(Math.round(img.cells[0] * img.ppc), Math.round(img.cells[1] * img.ppc));
  }
  const info = await pipe.png({ compressionLevel: 9 }).toFile(join(out, `${name}.png`));
  if (info.width > ATLAS_SIDE || info.height > ATLAS_SIDE) throw new Error(`${name} шире листа атласа: ${info.width}×${info.height}`);
  console.log(name, `${info.width}×${info.height}`);
}
