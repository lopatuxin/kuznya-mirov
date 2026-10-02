import type { ProjectFileReader } from "../projectLoader";
import { changedMaskPaths, type MaskBytes, type MaskSet } from "./maskBytes";
import type { Vec2 } from "./objectPlacement";
import { areCoversEqual, canPaintMaterial } from "./paintLayers";
import { applyPaintFrame, finishPaintStroke, startPaintStroke } from "./paintStroke";
import { decodeMaskPng, encodeMaskPng } from "./pngCodec";
import { chooseTerrainFilePath, parseProjectFilePaths, parseProjectMaterialNames, parseProjectStamps } from "./projectFiles";
import { parseSceneSize, type SceneSize } from "./sceneObjects";
import { addTerrainFilePath } from "./sceneTextEditing";
import { parseStrokeFile, type BrushStroke } from "./strokeFile";
import { strokeFrames } from "./strokePlayback";
import { applyBrushFrame, brushPathPoints, sampleGridHeight, syncFileHeights, type BrushGrid, type BrushKind } from "./terrainBrush";
import {
  differsInHundredths,
  parseTerrainText,
  roundToHundredths,
  terrainTextWithCovers,
  terrainTextWithHeights,
  type TerrainCoverLayer,
} from "./terrainFile";
import { readTerrainSnapshot } from "./terrainReadings";

/** Файл, который команда пишет после всех мазков: текст рельефа и `game.json` или байты PNG маски. */
export type StrokeWrite = { path: string; content: string | Uint8Array };

export type StrokeRunResult = { status: "ok"; message: string; writes: StrokeWrite[] } | { status: "error"; message: string };

/** `terrain_readings` движка: тексты `game.json`, файла рельефа и штампов — рельеф без видеокарты, ответ `{error}` или рельеф как у `terrain_heights`. */
export type TerrainReadingsFunction = (gameJsonText: string, terrainText: string | null, stamps: { name: string; text: string | null }[]) => unknown;

/** Что мазки меняют: высоты файла, покрытия и их маски; итоговая земля — высоты файла плюс горы. */
type StrokeWorld = {
  sceneSize: SceneSize;
  density: number;
  columns: number;
  rows: number;
  fileHeights: Float64Array;
  /** На сколько горы поднимают землю в каждой точке сетки: мазки гор не меняют. */
  mountainLift: Float64Array;
  covers: TerrainCoverLayer[] | null;
  tintPath: string | null;
  masks: MaskSet;
  isTerrainChanged: boolean;
};

class StrokeCommandError extends Error {}

function fail(message: string): never {
  throw new StrokeCommandError(message);
}

async function readMasks(files: ProjectFileReader, covers: readonly TerrainCoverLayer[] | null): Promise<MaskSet> {
  const masks: Record<string, MaskBytes> = {};
  for (const path of covers?.flatMap((layer) => (layer.mask === undefined ? [] : [layer.mask])) ?? []) {
    const bytes = await files.readBinary(path);
    if (bytes === null) fail(`${path}: файла нет`);
    try {
      masks[path] = await decodeMaskPng(bytes);
    } catch (error) {
      fail(`${path}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  return masks;
}

function strokeSteps(stroke: BrushStroke, applyFrame: (path: Vec2[], seconds: number) => void): void {
  let last = stroke.points[0] as Vec2;
  for (const frame of strokeFrames(stroke.points, stroke.seconds)) {
    applyFrame(brushPathPoints(last, frame.point, stroke.size), frame.seconds);
    last = frame.point;
  }
}

/** Мазок кисти рельефа: итоговая земля лепится, в файл идёт разница — как у мазка мышью («Лепка рельефа», требование 20). */
function runTerrainStroke(world: StrokeWorld, stroke: BrushStroke): void {
  const startEffective = Float64Array.from(world.fileHeights, (height, index) => height + (world.mountainLift[index] as number));
  const grid: BrushGrid = { density: world.density, columns: world.columns, rows: world.rows, heights: Float64Array.from(startEffective) };
  const levelTarget = sampleGridHeight(grid, startEffective, stroke.points[0] as Vec2);
  const settings = { kind: stroke.brush as BrushKind, size: stroke.size, strength: stroke.strength };
  strokeSteps(stroke, (path, seconds) => applyBrushFrame(grid, path, { settings, seconds, isLowering: stroke.isShift, levelTarget }));
  const heights = Float64Array.from(world.fileHeights);
  syncFileHeights(heights, world.fileHeights, grid.heights, startEffective);
  if (!differsInHundredths(world.fileHeights, heights)) return;
  world.fileHeights = Float64Array.from(heights, roundToHundredths);
  world.isTerrainChanged = true;
}

function runPaintStroke(world: StrokeWorld, stroke: BrushStroke, number: number): void {
  const material = stroke.material as string;
  if (!canPaintMaterial(world.covers, material)) fail(`мазок ${number} → material: слоёв уже восемь, а материала «${material}» среди них нет`);
  const paint = startPaintStroke(world.covers, world.masks, world.sceneSize, material, world.tintPath);
  if (paint === null) fail(`мазок ${number}: у слоя нет маски`);
  strokeSteps(stroke, (path, seconds) => applyPaintFrame(paint, path, { size: stroke.size, strength: stroke.strength, seconds, isErasing: stroke.isShift }));
  const result = finishPaintStroke(paint);
  if (result === null) return;
  world.isTerrainChanged = world.isTerrainChanged || !areCoversEqual(world.covers, result.covers);
  world.covers = result.covers;
  world.masks = { ...world.masks, ...result.masks };
}

async function readStampTexts(files: ProjectFileReader, gameJsonText: string): Promise<{ name: string; text: string | null }[]> {
  return Promise.all(parseProjectStamps(gameJsonText).map(async ({ name, path }) => ({ name, text: await files.readText(path) })));
}

async function openWorld(files: ProjectFileReader, readTerrain: TerrainReadingsFunction, gameJsonText: string, terrainText: string | null): Promise<StrokeWorld> {
  const readings = readTerrain(gameJsonText, terrainText, await readStampTexts(files, gameJsonText));
  const error = (readings as { error?: unknown } | undefined)?.error;
  if (typeof error === "string") fail(error);
  const snapshot = readTerrainSnapshot(readings);
  const sceneSize = parseSceneSize(gameJsonText);
  if (snapshot === undefined || sceneSize === null) fail("game.json: движок не отдал рельеф сцены");
  const parsed = terrainText === null ? null : parseTerrainText(terrainText);
  if (terrainText !== null && parsed === null) fail("файл рельефа не разбирается");
  const covers = parsed?.covers ?? null;
  const { grid, effective } = snapshot;
  return {
    sceneSize,
    density: grid.density,
    columns: grid.columns,
    rows: grid.rows,
    fileHeights: Float64Array.from(grid.heights),
    mountainLift: Float64Array.from(effective, (height, index) => height - (grid.heights[index] as number)),
    covers,
    tintPath: parsed?.tint ?? null,
    masks: await readMasks(files, covers),
    isTerrainChanged: false,
  };
}

async function collectWrites(files: ProjectFileReader, gameJsonText: string, terrainText: string | null, world: StrokeWorld, initialMasks: MaskSet): Promise<StrokeWrite[]> {
  const writes: StrokeWrite[] = [];
  for (const path of changedMaskPaths(initialMasks, world.masks)) writes.push({ path, content: await encodeMaskPng(world.masks[path] as MaskBytes) });
  if (!world.isTerrainChanged) return writes;
  const paths = parseProjectFilePaths(gameJsonText);
  let text = terrainTextWithHeights(terrainText, { columns: world.columns, rows: world.rows, heights: world.fileHeights });
  if (world.covers !== null) text = terrainTextWithCovers(text, world.sceneSize, world.covers) ?? text;
  if (paths?.terrain != null) return [...writes, { path: paths.terrain, content: text }];
  const terrainPath = await chooseTerrainFilePath(paths?.scene ?? "scene.json", async (path) => (await files.readText(path)) !== null);
  return [...writes, { path: terrainPath, content: text }, { path: "game.json", content: addTerrainFilePath(gameJsonText, terrainPath) }];
}

/**
 * Команда мазков — «Кисти», «Мазки командой», `npm run stroke`, требования 22–27: читает проект, выполняет мазки по
 * порядку тем же кодом, что мазки мыши в редакторе, и отдаёт файлы, которые надо записать. Ошибка где угодно — ничего
 * не записывается: файлы отдаются только после всех мазков.
 */
export async function runStrokes(files: ProjectFileReader, readTerrain: TerrainReadingsFunction, strokeFileText: string): Promise<StrokeRunResult> {
  try {
    const gameJsonText = await files.readText("game.json");
    if (gameJsonText === null) fail("game.json: файла нет");
    const terrainPath = parseProjectFilePaths(gameJsonText)?.terrain ?? null;
    const terrainText = terrainPath === null ? null : await files.readText(terrainPath);
    const world = await openWorld(files, readTerrain, gameJsonText, terrainText);
    const parsedStrokes = parseStrokeFile(strokeFileText, parseProjectMaterialNames(gameJsonText));
    if (parsedStrokes.status === "error") fail(parsedStrokes.message);
    if (parsedStrokes.strokes.length === 0) return { status: "ok", message: "Мазков нет", writes: [] };
    const initialMasks = world.masks;
    for (const [index, stroke] of parsedStrokes.strokes.entries()) {
      if (stroke.brush === "paint") runPaintStroke(world, stroke, index + 1);
      else runTerrainStroke(world, stroke);
    }
    const writes = await collectWrites(files, gameJsonText, terrainText, world, initialMasks);
    const message = writes.length === 0 ? "Мазки выполнены, изменений нет: ничего не записано" : `Записано: ${writes.map((write) => write.path).join(", ")}`;
    return { status: "ok", message, writes };
  } catch (error) {
    if (error instanceof StrokeCommandError) return { status: "error", message: error.message };
    throw error;
  }
}
