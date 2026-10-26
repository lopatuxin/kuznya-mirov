/**
 * Первый кадр видео: цвет верхней половины файла, прозрачность — яркость нижней («Картинки» → «Видео»). Редактор рисует по
 * нему карточку, движок берёт непрозрачные точки, с которых срываются листья («Ветер и частицы» → «Листопад»).
 */
export type VideoFirstFrame = { width: number; height: number; pixels: Uint8Array };

/**
 * Ответ по файлу `reverse` видео панели: проигрыватель только при `ok`. Размер и длину файла движок читает у самого
 * проигрывателя и сверяет с обычным сам («Картинки» → «Видео на панели»).
 */
export type DecodedReverseVideo = { verdict: "ok"; player: HTMLVideoElement } | { verdict: "missing" | "rejected" };

/**
 * Ровно то, что «load» ждёт по одному видео: при `ok` размер файла (оба слоя вместе) и проигрыватель,
 * при `missing`/`rejected` движок эти поля не читает. Из `frame` движку уходят только точки. `reverse` есть только у
 * видео, объявленного с `reverse`.
 */
export type DecodedVideo =
  | { verdict: "ok"; width: number; height: number; player: HTMLVideoElement; frame: VideoFirstFrame; reverse?: DecodedReverseVideo }
  | { verdict: "missing" | "rejected" };

const RED_WEIGHT = 0.299;
const GREEN_WEIGHT = 0.587;
const BLUE_WEIGHT = 0.114;
const RGBA_CHANNELS = 4;

/** Кадр из точек всего файла: цвет верхней половины, прозрачность — яркость точки под ней в нижней. */
export function composeFirstFrame(fileRgba: Uint8ClampedArray, fileWidth: number, fileHeight: number): VideoFirstFrame {
  const frameHeight = Math.floor(fileHeight / 2);
  const pointCount = fileWidth * frameHeight;
  const maskOffset = pointCount * RGBA_CHANNELS;
  const pixels = new Uint8Array(pointCount * RGBA_CHANNELS);
  for (let point = 0; point < pointCount; point += 1) {
    const color = point * RGBA_CHANNELS;
    const mask = maskOffset + color;
    pixels[color] = fileRgba[color] as number;
    pixels[color + 1] = fileRgba[color + 1] as number;
    pixels[color + 2] = fileRgba[color + 2] as number;
    pixels[color + 3] = Math.round(
      RED_WEIGHT * (fileRgba[mask] as number) + GREEN_WEIGHT * (fileRgba[mask + 1] as number) + BLUE_WEIGHT * (fileRgba[mask + 2] as number),
    );
  }
  return { width: fileWidth, height: frameHeight, pixels };
}

/** Освобождает проигрыватель: пауза, снятый источник (загрузка без `src` — не ошибка в консоли), отозванный Blob-адрес. */
export function releaseVideoPlayer(player: HTMLVideoElement): void {
  const url = player.src;
  player.pause();
  player.removeAttribute("src");
  player.load();
  URL.revokeObjectURL(url);
}

/** Освобождает проигрыватель обратного файла, если браузер взялся его играть. */
function releaseReverseVideo(reverse: DecodedReverseVideo | undefined): void {
  if (reverse?.verdict === "ok") releaseVideoPlayer(reverse.player);
}

/** Освобождает проигрыватели тех видео, что браузер взялся играть, — и обычные, и обратные. */
export function releaseDecodedVideos(videos: readonly DecodedVideo[]): void {
  for (const video of videos) {
    if (video.verdict !== "ok") continue;
    releaseVideoPlayer(video.player);
    releaseReverseVideo(video.reverse);
  }
}

/**
 * Проигрыватели, что сейчас держит движок: новая загрузка вызывает `replace` с новыми, и прежние освобождаются —
 * движок при `load` их только ставит на паузу и забывает («Картинки» → «Видео»).
 */
export type VideoPlayerKeeper = {
  replace: (next: readonly HTMLVideoElement[]) => void;
  releaseAll: () => void;
};

export function createVideoPlayerKeeper(): VideoPlayerKeeper {
  let held: readonly HTMLVideoElement[] = [];
  function replace(next: readonly HTMLVideoElement[]): void {
    const previous = held;
    held = next;
    for (const player of previous) releaseVideoPlayer(player);
  }
  return { replace, releaseAll: () => replace([]) };
}

/**
 * Первый кадр видео из памяти браузер отдаёт за доли секунды. Если за это время нет ни кадра, ни ошибки (упёрся в
 * предел числа проигрывателей, завис на разборе файла), видео считается тем, что браузер играть не может: иначе
 * загрузка проекта так и висела бы без сообщения.
 */
export const FIRST_FRAME_WAIT_MS = 15_000;

function waitForFirstFrame(player: HTMLVideoElement): Promise<boolean> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(false), FIRST_FRAME_WAIT_MS);
    const settle = (hasFrame: boolean) => () => {
      clearTimeout(timer);
      resolve(hasFrame);
    };
    player.addEventListener("loadeddata", settle(true), { once: true });
    player.addEventListener("error", settle(false), { once: true });
  });
}

function readFirstFrame(player: HTMLVideoElement): VideoFirstFrame | null {
  const width = player.videoWidth;
  const height = player.videoHeight;
  if (width === 0 || height === 0) return null;
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) return null;
  context.drawImage(player, 0, 0);
  return composeFirstFrame(context.getImageData(0, 0, width, height).data, width, height);
}

/** Проигрыватель без звука, по кругу, не показанный на странице, на Blob-адресе байтов; `hasFirstFrame` — дождался ли он первого кадра. */
function openVideoPlayer(bytes: Uint8Array): { player: HTMLVideoElement; hasFirstFrame: Promise<boolean> } {
  const player = document.createElement("video");
  player.muted = true;
  // Запасной круг для видео без длины. У видео панели по кругу движок сам снимает `loop` и ведёт два проигрывателя
  // того же файла по очереди, поэтому своего обработчика `timeupdate`/`ended` здесь нет: он боролся бы с этим ходом.
  player.loop = true;
  player.playsInline = true;
  player.preload = "auto";
  const hasFirstFrame = waitForFirstFrame(player);
  player.src = URL.createObjectURL(new Blob([bytes as BlobPart], { type: "video/mp4" }));
  return { player, hasFirstFrame };
}

/**
 * Обратный файл видео панели: проигрыватель такой же, как у основного видео, и тоже с первым кадром, но точки кадра
 * не читаются — ими покрашен только обычный файл. `null` — файл не нашёлся, это `missing`.
 */
export async function decodeReverseVideo(bytes: Uint8Array | null): Promise<DecodedReverseVideo> {
  if (bytes === null) return { verdict: "missing" };
  const { player, hasFirstFrame } = openVideoPlayer(bytes);
  if (!(await hasFirstFrame)) {
    releaseVideoPlayer(player);
    return { verdict: "rejected" };
  }
  return { verdict: "ok", player };
}

/**
 * `bytes === null` — файл не нашёлся, это `missing`. Иначе байты становятся Blob-адресом проигрывателя без звука,
 * по кругу, не показанного на странице; он ждёт первый кадр, и если браузер вместо него отдал ошибку или не ответил
 * за `FIRST_FRAME_WAIT_MS` — это `rejected`.
 * `reverseBytes` — файл `reverse` видео панели (`undefined` — не объявлен); он разбирается тем же порядком, а при
 * `missing` или `rejected` обычного файла его проигрыватель освобождается: играть им некому.
 * Играть и ставить на паузу проигрыватель будет сам движок.
 */
export async function decodeVideo(bytes: Uint8Array | null, reverseBytes?: Uint8Array | null): Promise<DecodedVideo> {
  if (bytes === null) return { verdict: "missing" };

  const { player, hasFirstFrame } = openVideoPlayer(bytes);
  const reverseDecoding = reverseBytes === undefined ? undefined : decodeReverseVideo(reverseBytes);

  let frame: VideoFirstFrame | null;
  try {
    frame = (await hasFirstFrame) ? readFirstFrame(player) : null;
  } catch (error) {
    // Чтение кадра упало (не хватило памяти под холст) — проигрыватель с источником не должен остаться играть.
    releaseVideoPlayer(player);
    releaseReverseVideo(await reverseDecoding);
    throw error;
  }
  const reverse = await reverseDecoding;
  if (frame === null) {
    releaseVideoPlayer(player);
    releaseReverseVideo(reverse);
    return { verdict: "rejected" };
  }
  return { verdict: "ok", width: player.videoWidth, height: player.videoHeight, player, frame, ...(reverse && { reverse }) };
}
