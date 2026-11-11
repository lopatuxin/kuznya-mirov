import { readFile, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const sharp = require('../../../../tools/art/node_modules/sharp');
const directory = path.dirname(fileURLToPath(import.meta.url));
const heroDirectory = path.resolve(directory, '..');
const fontDirectory = path.resolve(directory, '../../../../games/platformer/fonts');
const gameImageDirectory = path.resolve(directory, '../../../../games/platformer/images');
const width = 2560;
const height = 1080;
const side = (width - 1920) / 2;
const characters = {
  ilya: { image: 'ilya_front_axe_ground_v2.png', name: 'Илья Муромец', role: 'СИЛА' },
  alyosha: { image: 'alyosha_front_v2.png', name: 'Алёша Попович', role: 'ХИТРОСТЬ' },
  vasilisa: { image: 'vasilisa_front_v1.png', name: 'Василиса Премудрая', role: 'МУДРОСТЬ' },
};

function shapes(markup) {
  return Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${markup}</svg>`);
}

async function text(value, x, y, size, color, font = 'Monomakh', align = 'center') {
  const fontfile = path.join(fontDirectory, `${font}.ttf`);
  const input = await sharp({ text: {
    text: `<span foreground="${color}">${value}</span>`,
    font: `${font} ${size}`,
    fontfile,
    rgba: true,
  } }).png().toBuffer();
  const metadata = await sharp(input).metadata();
  return { input, left: align === 'center' ? Math.round(x + side - metadata.width / 2) : x + side, top: y };
}

async function figure(key, centerX, bottom, figureHeight) {
  const input = await sharp(path.join(heroDirectory, characters[key].image)).resize({ height: figureHeight }).png().toBuffer();
  const metadata = await sharp(input).metadata();
  return { input, left: Math.round(centerX + side - metadata.width / 2), top: bottom - metadata.height };
}

async function render(active, left, right) {
  const backdrop = await sharp(path.join(directory, 'hall_bg_wide.png')).resize(width, height, { fit: 'fill' }).png().toBuffer();
  const stage = shapes(`<defs><linearGradient id="top" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#20131c" stop-opacity=".87"/><stop offset="1" stop-color="#20131c" stop-opacity="0"/></linearGradient></defs>
    <rect x="0" y="0" width="${width}" height="160" fill="url(#top)"/><rect x="0" y="174" width="${side + 615}" height="698" fill="#1c1422" fill-opacity=".18"/><rect x="${side + 1300}" y="174" width="${side + 620}" height="698" fill="#1c1422" fill-opacity=".18"/>
    <ellipse cx="${side + 345}" cy="860" rx="155" ry="30" fill="#1b1121" fill-opacity=".48"/><ellipse cx="${side + 1575}" cy="860" rx="155" ry="30" fill="#1b1121" fill-opacity=".48"/>
    <ellipse cx="${side + 960}" cy="545" rx="260" ry="335" fill="#ffd994" fill-opacity=".23"/><ellipse cx="${side + 960}" cy="875" rx="258" ry="52" fill="#f8d184" fill-opacity=".27"/>`);
  const plaques = shapes(`<g transform="translate(${side} 0)"><rect x="92" y="892" width="505" height="99" rx="5" fill="#35242b" fill-opacity=".91" stroke="#9f7958" stroke-width="2"/>
    <rect x="1323" y="892" width="505" height="99" rx="5" fill="#35242b" fill-opacity=".91" stroke="#9f7958" stroke-width="2"/>
    <rect x="627" y="876" width="666" height="146" rx="6" fill="#6b3427" fill-opacity=".96" stroke="#f7d284" stroke-width="5"/>
    <path d="M648 567 l-23 24 23 24 M1272 567 l23 24 -23 24" fill="none" stroke="#f7d284" stroke-width="7" stroke-linecap="round" stroke-linejoin="round"/></g>`);
  const layers = [
    { input: stage, left: 0, top: 0 },
    await figure(left, 345, 875, 570),
    await figure(right, 1575, 875, 570),
    await figure(active, 960, 875, 745),
    { input: plaques, left: 0, top: 0 },
    await text('Выберите героя', 960, 28, 77, '#fff1cf'),
    await text(characters[left].name, 345, 907, 40, '#ead6b6'),
    await text(characters[right].name, 1575, 907, 40, '#ead6b6'),
    await text(characters[active].name, 960, 887, active === 'vasilisa' ? 54 : 57, '#fff1cf'),
    await text(characters[left].role, 345, 961, 18, '#c6a57d', 'Rubik'),
    await text(characters[right].role, 1575, 961, 18, '#c6a57d', 'Rubik'),
    await text(`${characters[active].role}  ·  ПОДРОБНЕЕ →`, 960, 958, 21, '#f2d49c', 'Rubik'),
    await text('← Назад', 76, 1030, 29, '#ffdfab', 'Monomakh', 'left'),
  ];
  const composed = await sharp(backdrop).composite(layers).png().toBuffer();
  const output = path.join(gameImageDirectory, `hero_select_hall_${active}_wide.png`);
  await rm(output, { force: true });
  await writeFile(output, composed);
}

await Promise.all(Object.values(characters).map((character) => readFile(path.join(heroDirectory, character.image))));
await render('ilya', 'vasilisa', 'alyosha');
await render('alyosha', 'ilya', 'vasilisa');
await render('vasilisa', 'ilya', 'alyosha');
