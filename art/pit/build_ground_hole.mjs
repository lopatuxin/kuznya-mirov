// Плита нарисованной земли Ямы с настоящей норой посреди дна: зомби, что лезет из неё, виден через отверстие
// на её стенках, а ближний край норы закрывает его ноги. Картина та же, что у плиты build_ground.py; каждая точка
// стенки берёт цвет той точки картины, в которую её видит камера игры: ортографическая камера с наклоном 45°
// показывает точку на глубине d там же, где точку земли на d ближе к игроку.
// Движок ставит самую низкую точку модели на землю, поэтому плита стоит в сцене на высоте −DEPTH.
// node art/pit/build_ground_hole.mjs [исходная плита] [куда записать]
import fs from 'fs';

const here = (p) => new URL(p, import.meta.url).pathname.replace(/^\/(\w:)/, '$1');
const SOURCE = process.argv[2] ?? here('../../games/pit/models/ground.glb');
const TARGET = process.argv[3] ?? SOURCE;
const PICTURE_W = 2117, PICTURE_H = 941;
const WIDTH = 44.5424690246582, LENGTH = 28;
// Отверстие норы на картине (точки картины): внутренний край комьев вокруг тёмной стенки.
export const HOLE = { x: 1068, y: 490, radius: 1.45 };
export const DEPTH = 6.2;
const SEGMENTS = 48;

const cx = (HOLE.x / PICTURE_W) * WIDTH, cy = (HOLE.y / PICTURE_H) * LENGTH;
// Точка сцены (x вправо, y к игроку, h вверх) → вершина glTF (x, h, y) и место на картине.
const positions = [], normals = [], uvs = [], indices = [];
function vertex(x, y, h, normal) {
  positions.push([x, h + DEPTH, y]);
  normals.push(normal);
  const seen = y - h; // точка ниже земли видна там же, где земля ближе к игроку
  uvs.push([x / WIDTH, Math.min(1, Math.max(0, seen / LENGTH))]);
  return positions.length - 1;
}
const quad = (a, b, c, d) => indices.push(a, b, c, a, c, d);

// Направления лучей из середины норы: равные доли круга и углы плиты — тогда отрезок края плиты между соседними
// лучами лежит на одной стороне, и плита собирается из четырёхугольников без потерь у углов.
const corners = [[0, 0], [WIDTH, 0], [WIDTH, LENGTH], [0, LENGTH]].map(([x, y]) => Math.atan2(y - cy, x - cx));
const angles = [...Array(SEGMENTS)].map((_, i) => (i / SEGMENTS) * 2 * Math.PI - Math.PI).concat(corners).sort((a, b) => a - b);
function edgeHit(a) {
  const dx = Math.cos(a), dy = Math.sin(a);
  const ts = [];
  if (dx > 1e-9) ts.push((WIDTH - cx) / dx); else if (dx < -1e-9) ts.push(-cx / dx);
  if (dy > 1e-9) ts.push((LENGTH - cy) / dy); else if (dy < -1e-9) ts.push(-cy / dy);
  const t = Math.min(...ts);
  return [cx + dx * t, cy + dy * t];
}
const up = [0, 1, 0];
const ring = angles.map((a) => {
  const [ex, ey] = edgeHit(a);
  const rx = cx + Math.cos(a) * HOLE.radius, ry = cy + Math.sin(a) * HOLE.radius;
  const inward = [-Math.cos(a), 0, -Math.sin(a)];
  return {
    edge: vertex(ex, ey, 0, up),
    rim: vertex(rx, ry, 0, up),
    top: vertex(rx, ry, 0, inward),
    bottom: vertex(rx, ry, -DEPTH, inward),
  };
});
const floorCentre = vertex(cx, cy, -DEPTH, up);
for (let i = 0; i < ring.length; i++) {
  const a = ring[i], b = ring[(i + 1) % ring.length];
  quad(a.rim, b.rim, b.edge, a.edge);
  quad(a.top, a.bottom, b.bottom, b.top);
  indices.push(floorCentre, b.bottom, a.bottom);
}

// Запись: картина и материал — из исходной плиты, сетка — новая.
const raw = fs.readFileSync(SOURCE);
const jsonLength = raw.readUInt32LE(12);
const doc = JSON.parse(raw.slice(20, 20 + jsonLength));
const bin = raw.slice(28 + jsonLength);
const image = doc.images[0];
const imageView = doc.bufferViews[image.bufferView];
const pictureBytes = bin.slice(imageView.byteOffset ?? 0, (imageView.byteOffset ?? 0) + imageView.byteLength);
const chunks = [];
let offset = 0;
const views = [], accessors = [];
function add(buffer, target) {
  const padded = Buffer.alloc(Math.ceil(buffer.length / 4) * 4);
  buffer.copy(padded);
  views.push({ buffer: 0, byteOffset: offset, byteLength: buffer.length, ...(target ? { target } : {}) });
  chunks.push(padded);
  offset += padded.length;
  return views.length - 1;
}
function floats(rows, type) {
  const buffer = Buffer.alloc(rows.length * rows[0].length * 4);
  rows.flat().forEach((v, k) => buffer.writeFloatLE(v, k * 4));
  const width = rows[0].length;
  accessors.push({
    bufferView: add(buffer, 34962), componentType: 5126, count: rows.length, type,
    min: [...Array(width)].map((_, c) => Math.min(...rows.map((r) => r[c]))),
    max: [...Array(width)].map((_, c) => Math.max(...rows.map((r) => r[c]))),
  });
  return accessors.length - 1;
}
const position = floats(positions, 'VEC3');
const normal = floats(normals, 'VEC3');
const uv = floats(uvs, 'VEC2');
const indexBuffer = Buffer.alloc(indices.length * 4);
indices.forEach((v, k) => indexBuffer.writeUInt32LE(v, k * 4));
accessors.push({ bufferView: add(indexBuffer, 34963), componentType: 5125, count: indices.length, type: 'SCALAR' });
const indexAccessor = accessors.length - 1;
const pictureView = add(pictureBytes);
const out = {
  ...doc,
  meshes: [{ name: 'pit_ground', primitives: [{ attributes: { POSITION: position, NORMAL: normal, TEXCOORD_0: uv }, indices: indexAccessor, material: 0 }] }],
  accessors,
  bufferViews: views,
  buffers: [{ byteLength: offset }],
  images: [{ ...image, bufferView: pictureView }],
};
let json = Buffer.from(JSON.stringify(out));
json = Buffer.concat([json, Buffer.alloc((4 - (json.length % 4)) % 4, 0x20)]);
const body = Buffer.concat(chunks);
const header = Buffer.alloc(12);
header.writeUInt32LE(0x46546c67, 0); header.writeUInt32LE(2, 4);
header.writeUInt32LE(12 + 8 + json.length + 8 + body.length, 8);
const chunkHeader = (len, type) => { const b = Buffer.alloc(8); b.writeUInt32LE(len, 0); b.writeUInt32LE(type, 4); return b; };
fs.writeFileSync(TARGET, Buffer.concat([header, chunkHeader(json.length, 0x4e4f534a), json, chunkHeader(body.length, 0x004e4942), body]));
console.log(JSON.stringify({ target: TARGET, vertices: positions.length, triangles: indices.length / 3, hole: { x: +cx.toFixed(3), y: +cy.toFixed(3), radius: HOLE.radius }, depth: DEPTH }));
