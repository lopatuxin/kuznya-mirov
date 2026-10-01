import { describe, expect, it, vi } from "vitest";
import type { HandleMode } from "./handleGeometry";
import { mountainHandleGeometry, readMountain, type Mountain } from "./mountainGeometry";
import {
  createMountainSceneController,
  type MountainContext,
  type MountainPlacement,
  type MountainSceneContext,
  type MountainSceneEngine,
} from "./mountainSceneController";
import type { Vec2 } from "./objectPlacement";
import { pinholeCamera } from "./pinholeCamera";
import type { MountainEntry } from "./terrainFile";

const FIRST: MountainEntry = { stamp: "beluha", position: [20, 15], size: [20, 10], height: 6 };
const SECOND: MountainEntry = { stamp: "chuya", position: [40, 30], size: [10, 10], height: 3, rotation: 30 };
const FILE_HEIGHTS = new Float64Array(25).fill(0.5);

type Pointer = { pointerId: number; button: number; buttons: number; x: number; y: number; ctrlKey: boolean };

function pointer(at: Vec2, extra: Partial<Pointer> = {}): Pointer {
  return { pointerId: 1, button: 0, buttons: 1, x: at[0], y: at[1], ctrlKey: false, ...extra };
}

function release(at: Vec2, extra: Partial<Pointer> = {}): Pointer {
  return pointer(at, { buttons: 0, ...extra });
}

type SetupOptions = { pitch?: number; handleMode?: HandleMode; selectedIndex?: number | null; placing?: MountainPlacement | null; isEditable?: boolean; hasBrush?: boolean };

function setup(options: SetupOptions = {}) {
  const camera = pinholeCamera([30, 20], 15, options.pitch ?? 55, 60);
  const state = {
    /** Высота земли под указателем: начинается жест на ней, дальше земля меняется как хочет. */
    groundZ: 0,
    isSky: false,
    pickedStamp: undefined as number | undefined,
    setError: undefined as string | undefined,
  };
  const setCalls: { heights: Float64Array; water: unknown; stamps: MountainEntry[] }[] = [];
  const groundUnder = (at: Vec2, z: number): [number, number] => {
    const direction = camera.rayDirection(at);
    const depth = (z - camera.eye[2]) / (direction[2] as number);
    return [camera.eye[0] + depth * (direction[0] as number), camera.eye[1] + depth * (direction[1] as number)];
  };
  const engine = {
    screen_point: (x: number, y: number, z: number) => camera.projection.screenPoint(x, y, z),
    terrain_height: () => 0,
    terrain_at: (x: number, y: number) => {
      const place = groundUnder([x, y], state.groundZ);
      return state.isSky ? undefined : [place[0], place[1], state.groundZ];
    },
    terrain_heights: () => ({ density: 2, columns: 5, rows: 5, heights: Float64Array.from(FILE_HEIGHTS), effective: Float64Array.from(FILE_HEIGHTS), water: { level: -1, color: "#112233" } }),
    set_terrain: (heights: Float64Array, water: unknown, stamps: MountainEntry[]) => {
      setCalls.push({ heights: Float64Array.from(heights), water, stamps });
      return state.setError;
    },
    stamp_at: () => state.pickedStamp,
  } as unknown as MountainSceneEngine;

  const selections: number[] = [];
  const placed: MountainEntry[] = [];
  const commits: [number, MountainEntry][] = [];
  const activity: boolean[] = [];
  const mountains: MountainContext = {
    isEditable: options.isEditable ?? true,
    entries: [FIRST, SECOND],
    selectedIndex: options.selectedIndex === undefined ? 0 : options.selectedIndex,
    placing: options.placing ?? null,
    onSelect: (index) => selections.push(index),
    onPlace: (entry) => placed.push(entry),
    onCommit: (index, entry) => commits.push([index, entry]),
    onActiveChange: (isActive) => activity.push(isActive),
  };
  const context: MountainSceneContext = {
    engine,
    handleMode: options.handleMode ?? "translate",
    brush: options.hasBrush === true ? { kind: "raise", size: 4, strength: 50 } : null,
    mountains,
  };
  const controller = createMountainSceneController();
  const screen = (x: number, y: number, z = 0): Vec2 => camera.projection.screenPoint(x, y, z) as Vec2;
  const geometry = (mode: HandleMode, mountain: Mountain = readMountain(FIRST) as Mountain) => mountainHandleGeometry(camera.projection, mountain, mode, 0);
  return { controller, context, engine, state, setCalls, selections, placed, commits, activity, screen, geometry, groundUnder, camera };
}

/** Точка на отрезке от середины к концу стрелки: доля 1 — конец. */
function alongArrow(center: Vec2, tip: Vec2 | null, share: number): Vec2 {
  const end = tip as Vec2;
  return [center[0] + (end[0] - center[0]) * share, center[1] + (end[1] - center[1]) * share];
}

function lastMountain(scene: ReturnType<typeof setup>): Mountain {
  return readMountain(scene.setCalls.at(-1)?.stamps[0]) as Mountain;
}

describe("кнопка «Гора»", () => {
  const PLACING: MountainPlacement = { stamp: { name: "chuya", columns: 192, rows: 144 }, width: 30, height: 10 };

  it("щелчок по земле ставит гору: середина — место под указателем, глубина по пропорции штампа, rotation нет", () => {
    const scene = setup({ placing: PLACING });
    const at = scene.screen(22.126, 14.004);
    scene.controller.place(scene.context, pointer(at));
    expect(scene.placed).toHaveLength(1);
    const entry = scene.placed[0] as MountainEntry;
    expect(entry.stamp).toBe("chuya");
    expect((entry.position as number[])[0]).toBeCloseTo(22.13, 2);
    expect((entry.position as number[])[1]).toBeCloseTo(14, 2);
    expect(entry.size).toEqual([30, 22.5]);
    expect(entry.height).toBe(10);
    expect(entry).not.toHaveProperty("rotation");
  });

  it("луч мимо рельефа — горы нет", () => {
    const scene = setup({ placing: PLACING });
    scene.state.isSky = true;
    scene.controller.place(scene.context, pointer([100, 100]));
    expect(scene.placed).toEqual([]);
  });
});

describe("выбор горы щелчком", () => {
  it("гора под указателем выбирается и берётся за тело", () => {
    const scene = setup({ selectedIndex: null });
    scene.state.pickedStamp = 1;
    expect(scene.controller.pickAt(scene.context, pointer(scene.screen(40, 30)))).toBe(true);
    expect(scene.selections).toEqual([1]);
    expect(scene.activity).toEqual([true]);
  });

  it("гор под указателем нет — null: основной контроллер снимет выбор сам", () => {
    const scene = setup();
    expect(scene.controller.pickAt(scene.context, pointer([10, 10]))).toBe(null);
    expect(scene.selections).toEqual([]);
  });

  it("в партии, на паузе и в повторе горы не выбираются: движок и не спрашивается", () => {
    const scene = setup({ isEditable: false });
    scene.state.pickedStamp = 1;
    const stampAt = vi.spyOn(scene.engine, "stamp_at");
    expect(scene.controller.pickAt(scene.context, pointer([10, 10]))).toBe(null);
    expect(stampAt).not.toHaveBeenCalled();
  });

  it("гора, у которой не все поля числа, выбирается, но жеста нет", () => {
    const scene = setup({ selectedIndex: null });
    scene.context.mountains.entries = [{ stamp: "a", position: "abc", size: [1, 1], height: 1 }];
    scene.state.pickedStamp = 0;
    expect(scene.controller.pickAt(scene.context, pointer([10, 10]))).toBe(false);
    expect(scene.selections).toEqual([0]);
  });
});

describe("перенос за тело", () => {
  it("сдвиг дальше 4 точек начинает перенос: точка, за которую взялись, идёт за указателем по плоскости на своей высоте", () => {
    const scene = setup();
    scene.state.groundZ = 4;
    scene.state.pickedStamp = 0;
    const terrainAt = vi.spyOn(scene.engine, "terrain_at");
    expect(scene.controller.pickAt(scene.context, pointer(scene.screen(22, 16, 4)))).toBe(true);
    // Земля под указателем после этого может быть любой — плоскость на высоте 4 её не слушает.
    scene.state.groundZ = -3;
    expect(scene.controller.pointerMove(scene.context, pointer(scene.screen(25, 19, 4)))).toBe(true);
    expect(terrainAt).toHaveBeenCalledTimes(1);
    expect(scene.setCalls).toHaveLength(1);
    const moved = lastMountain(scene);
    expect(moved.position[0]).toBeCloseTo(23, 2);
    expect(moved.position[1]).toBeCloseTo(18, 2);
    // Горы остальные, высоты файла и вода — как на начало жеста.
    expect(scene.setCalls[0]?.stamps[1]).toBe(SECOND);
    expect(Array.from(scene.setCalls[0]?.heights ?? [])).toEqual(Array.from(FILE_HEIGHTS));
    expect(scene.setCalls[0]?.water).toEqual({ level: -1, color: "#112233" });
  });

  it("отпускание — одно действие с новой горой целиком; движение меньше 4 точек не действие", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    const start = scene.screen(22, 16);
    scene.controller.pickAt(scene.context, pointer(start));
    scene.controller.pointerMove(scene.context, pointer([start[0] + 3, start[1]]));
    expect(scene.setCalls).toEqual([]);
    scene.controller.pointerMove(scene.context, pointer(scene.screen(25, 19)));
    expect(scene.controller.pointerUp(scene.context, release(scene.screen(25, 19)))).toBe(true);
    expect(scene.commits).toHaveLength(1);
    const [index, entry] = scene.commits[0] as [number, MountainEntry];
    expect(index).toBe(0);
    expect(entry).toEqual({ stamp: "beluha", position: [23, 18], size: [20, 10], height: 6 });
    expect(scene.activity).toEqual([true, false]);
  });

  it("щелчок без движения ничего не пишет", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    const start = scene.screen(22, 16);
    scene.controller.pickAt(scene.context, pointer(start));
    scene.controller.pointerUp(scene.context, release(start));
    expect(scene.commits).toEqual([]);
    expect(scene.setCalls).toEqual([]);
    expect(scene.activity).toEqual([true, false]);
  });

  it("с Ctrl середина встаёт на целые клетки", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    scene.controller.pickAt(scene.context, pointer(scene.screen(22, 16)));
    scene.controller.pointerMove(scene.context, pointer(scene.screen(24.6, 18.4), { ctrlKey: true }));
    expect(lastMountain(scene).position).toEqual([23, 17]);
  });

  it("Esc возвращает гору на место: движок получает прежние горы, действия нет", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    scene.controller.pickAt(scene.context, pointer(scene.screen(22, 16)));
    scene.controller.pointerMove(scene.context, pointer(scene.screen(25, 19)));
    expect(scene.controller.cancel(scene.context)).toBe(true);
    expect(scene.setCalls.at(-1)?.stamps).toEqual([FIRST, SECOND]);
    expect(scene.controller.pointerUp(scene.context, release(scene.screen(25, 19)))).toBe(false);
    expect(scene.commits).toEqual([]);
    expect(scene.activity).toEqual([true, false]);
  });

  it("Esc без жеста — не жест горы", () => {
    expect(setup().controller.cancel(setup().context)).toBe(false);
  });

  it("сброс указателя браузером возвращает гору, как Esc", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    scene.controller.pickAt(scene.context, pointer(scene.screen(22, 16)));
    scene.controller.pointerMove(scene.context, pointer(scene.screen(25, 19)));
    expect(scene.controller.pointerCancel(scene.context, pointer([0, 0]))).toBe(true);
    expect(scene.setCalls.at(-1)?.stamps).toEqual([FIRST, SECOND]);
    expect(scene.commits).toEqual([]);
  });

  it("жест бросается извне: гора не возвращается, действия нет, страница узнаёт о конце жеста", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    scene.controller.pickAt(scene.context, pointer(scene.screen(22, 16)));
    scene.controller.pointerMove(scene.context, pointer(scene.screen(25, 19)));
    scene.controller.abandon();
    expect(scene.controller.pointerUp(scene.context, release(scene.screen(25, 19)))).toBe(false);
    expect(scene.setCalls).toHaveLength(1);
    expect(scene.commits).toEqual([]);
    expect(scene.activity).toEqual([true, false]);
  });

  it("движок отказал в set_terrain — жест бросается, действия нет", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    scene.state.setError = "stamps[0] → size: плохо";
    scene.controller.pickAt(scene.context, pointer(scene.screen(22, 16)));
    scene.controller.pointerMove(scene.context, pointer(scene.screen(25, 19)));
    expect(scene.controller.pointerUp(scene.context, release(scene.screen(25, 19)))).toBe(false);
    expect(scene.commits).toEqual([]);
    expect(scene.activity).toEqual([true, false]);
  });

  it("чужой указатель жест не трогает; отпускание, пока кнопка ещё зажата, жест не кончает", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    scene.controller.pickAt(scene.context, pointer(scene.screen(22, 16)));
    expect(scene.controller.pointerMove(scene.context, pointer(scene.screen(25, 19), { pointerId: 2 }))).toBe(true);
    expect(scene.setCalls).toEqual([]);
    expect(scene.controller.pointerUp(scene.context, pointer(scene.screen(25, 19), { buttons: 1 }))).toBe(true);
    expect(scene.controller.isActive()).toBe(true);
  });

  it("луч указателя не пересекает плоскость переноса — гора стоит, пока луч не вернётся", () => {
    const scene = setup({ pitch: 10 });
    scene.state.pickedStamp = 0;
    scene.controller.pickAt(scene.context, pointer(scene.screen(22, 16)));
    // Верх экрана при наклоне 10° смотрит выше горизонта.
    scene.controller.pointerMove(scene.context, pointer([640, 0]));
    expect(scene.setCalls).toEqual([]);
    scene.controller.pointerMove(scene.context, pointer(scene.screen(25, 19)));
    expect(scene.setCalls).toHaveLength(1);
  });

  it("камера, которую по трём осям не восстановить, — жеста нет", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    (scene.engine as { screen_point: unknown }).screen_point = () => undefined;
    expect(scene.controller.pickAt(scene.context, pointer([100, 100]))).toBe(false);
    expect(scene.activity).toEqual([]);
  });
});

describe("ручки выбранной горы", () => {
  it("стрелка оси x меняет одну координату", () => {
    const scene = setup();
    const geometry = scene.geometry("translate");
    const start = alongArrow(geometry?.center as Vec2, geometry?.tipX ?? null, 0.6);
    expect(scene.controller.startHandle(scene.context, pointer(start))).toBe(true);
    const ground = scene.groundUnder(start, 0);
    scene.controller.pointerMove(scene.context, pointer(scene.screen(ground[0] + 3, ground[1] + 2)));
    expect(lastMountain(scene).position[0]).toBeCloseTo(23, 2);
    expect(lastMountain(scene).position[1]).toBe(15);
  });

  it("середина двигает гору свободно по горизонтальной плоскости середины", () => {
    const scene = setup();
    const geometry = scene.geometry("translate");
    const start = geometry?.center as Vec2;
    expect(scene.controller.startHandle(scene.context, pointer(start))).toBe(true);
    scene.controller.pointerMove(scene.context, pointer(scene.screen(23, 18)));
    expect(lastMountain(scene).position[0]).toBeCloseTo(23, 2);
    expect(lastMountain(scene).position[1]).toBeCloseTo(18, 2);
  });

  it("у переноса вертикальной стрелки нет: нажатие там, где она была бы, ручку не берёт", () => {
    const scene = setup();
    const center = scene.geometry("translate")?.center as Vec2;
    expect(scene.controller.startHandle(scene.context, pointer([center[0], center[1] - 70]))).toBe(false);
    expect(scene.activity).toEqual([]);
  });

  it("поворот: кольцо вокруг вертикали, на 90° — rotation 90, с Ctrl кратно 15°", () => {
    const scene = setup({ handleMode: "rotate" });
    const geometry = scene.geometry("rotate");
    const start = geometry?.ring[0] as Vec2;
    expect(scene.controller.startHandle(scene.context, pointer(start))).toBe(true);
    const [gx, gy] = scene.groundUnder(start, 0);
    const [cx, cy] = [20, 15];
    scene.controller.pointerMove(scene.context, pointer(scene.screen(cx - (gy - cy), cy + (gx - cx))));
    expect(lastMountain(scene).rotation).toBe(90);
    const turned = scene.groundUnder(start, 0);
    const angle = 0.3;
    const [dx, dy] = [turned[0] - cx, turned[1] - cy];
    scene.controller.pointerMove(scene.context, pointer(scene.screen(cx + dx * Math.cos(angle) - dy * Math.sin(angle), cy + dx * Math.sin(angle) + dy * Math.cos(angle)), { ctrlKey: true }));
    expect(lastMountain(scene).rotation).toBe(15);
    scene.controller.pointerUp(scene.context, release(start));
    expect(scene.commits[0]?.[1]).toMatchObject({ rotation: 15 });
  });

  it("масштаб: стрелка ширины растягивает вдвое, середина на месте, высота прежняя", () => {
    const scene = setup({ handleMode: "scale" });
    const geometry = scene.geometry("scale");
    const center = geometry?.center as Vec2;
    expect(scene.controller.startHandle(scene.context, pointer(alongArrow(center, geometry?.tipX ?? null, 0.7)))).toBe(true);
    scene.controller.pointerMove(scene.context, pointer(alongArrow(center, geometry?.tipX ?? null, 1.4)));
    const scaled = lastMountain(scene);
    expect(scaled.size[0]).toBeCloseTo(40, 1);
    expect(scaled.size[1]).toBe(10);
    expect(scaled.position[0]).toBeCloseTo(20, 2);
    expect(scaled.position[1]).toBeCloseTo(15, 2);
    expect(scaled.height).toBe(6);
  });

  it("масштаб: зелёная ручка — высота горы", () => {
    const scene = setup({ handleMode: "scale" });
    const geometry = scene.geometry("scale");
    const center = geometry?.center as Vec2;
    expect(scene.controller.startHandle(scene.context, pointer(alongArrow(center, geometry?.tipZ ?? null, 0.7)))).toBe(true);
    scene.controller.pointerMove(scene.context, pointer(alongArrow(center, geometry?.tipZ ?? null, 1.4)));
    expect(lastMountain(scene).height).toBeCloseTo(12, 1);
    expect(lastMountain(scene).size).toEqual([20, 10]);
  });

  it("масштаб: общая ручка в середине меняет всё, не меньше 0,1 клетки", () => {
    const scene = setup({ handleMode: "scale" });
    const center = scene.geometry("scale")?.center as Vec2;
    expect(scene.controller.startHandle(scene.context, pointer(center))).toBe(true);
    scene.controller.pointerMove(scene.context, pointer([center[0] - 5000, center[1] + 5000]));
    expect(lastMountain(scene)).toMatchObject({ size: [0.1, 0.1], height: 0.1 });
  });

  it("при кисти, при «Горе» и не вне партии ручек нет", () => {
    for (const options of [{ hasBrush: true }, { placing: { stamp: { name: "a", columns: 2, rows: 2 }, width: 1, height: 1 } }, { isEditable: false }, { selectedIndex: null }]) {
      const scene = setup(options);
      const center = scene.geometry("translate")?.center as Vec2;
      expect(scene.controller.startHandle(scene.context, pointer(center))).toBe(false);
    }
  });
});

function recordingCanvas(): CanvasRenderingContext2D {
  const target: Record<string, unknown> = { canvas: { width: 800, height: 600 } };
  return new Proxy(target, {
    get: (record, name: string) => (name in record ? record[name] : () => ({ width: 10 })),
    set: (record, name: string, value: unknown) => {
      record[name] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

describe("рамка и ручки", () => {
  it("рамка — точки по сторонам на видимой земле, ручки — отдельно", () => {
    const scene = setup();
    const terrainHeight = vi.spyOn(scene.engine, "terrain_height");
    scene.controller.draw(scene.context, recordingCanvas(), 1);
    // 20 + 10 + 20 + 10 точек по сторонам; ручкам середина и концы стрелок.
    expect(terrainHeight.mock.calls.length).toBeGreaterThanOrEqual(60);
  });

  it("не выбрано, не вне партии — ничего не рисуется", () => {
    for (const options of [{ selectedIndex: null }, { isEditable: false }]) {
      const scene = setup(options);
      const screenPoint = vi.spyOn(scene.engine, "screen_point");
      scene.controller.draw(scene.context, recordingCanvas(), 1);
      expect(screenPoint).not.toHaveBeenCalled();
    }
  });

  it("при кисти рамка рисуется, а ручки нет", () => {
    const withHandles = setup();
    const withBrush = setup({ hasBrush: true });
    const handleCalls = vi.spyOn(withHandles.engine, "screen_point");
    const brushCalls = vi.spyOn(withBrush.engine, "screen_point");
    withHandles.controller.draw(withHandles.context, recordingCanvas(), 1);
    withBrush.controller.draw(withBrush.context, recordingCanvas(), 1);
    expect(brushCalls.mock.calls.length).toBe(60);
    expect(handleCalls.mock.calls.length).toBeGreaterThan(brushCalls.mock.calls.length);
  });

  it("во время жеста рамка идёт за горой на новом месте", () => {
    const scene = setup();
    scene.state.pickedStamp = 0;
    scene.controller.pickAt(scene.context, pointer(scene.screen(22, 16)));
    scene.controller.pointerMove(scene.context, pointer(scene.screen(25, 19)));
    const screenPoint = vi.spyOn(scene.engine, "screen_point");
    scene.controller.draw(scene.context, recordingCanvas(), 1);
    const outlineFirst = screenPoint.mock.calls[0] as [number, number, number];
    expect(outlineFirst[0]).toBeCloseTo(13, 1);
    expect(outlineFirst[1]).toBeCloseTo(13, 1);
  });
});
