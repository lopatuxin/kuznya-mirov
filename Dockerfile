# --- Stage 1: собрать ядро в WebAssembly ---
FROM rust:1-bookworm AS wasm-builder
WORKDIR /app/engine

# wasm-pack сам тянет wasm-opt (binaryen) из GitHub release при первом запуске,
# если не находит его в PATH — версия той загрузки Dockerfile-ом не закреплена.
# Ставим нужную версию binaryen явно и с проверкой контрольной суммы, чтобы
# wasm-pack нашёл готовый бинарник в PATH и сетевую загрузку не делал.
# Версию меняют только вместе с обеими контрольными суммами ниже, поэтому она
# объявлена здесь же, а не отдельным ARG: снаружи её переопределять нечем.
RUN set -eu; \
    BINARYEN_VERSION=117; \
    case "$(dpkg --print-architecture)" in \
      amd64) BINARYEN_ARCH=x86_64; BINARYEN_SHA256=3dc677006555b355ea2da5e82602065a161d5e83eaefd3f759afa00b96e83212 ;; \
      arm64) BINARYEN_ARCH=aarch64; BINARYEN_SHA256=ad560204426015a815faa45693c83bef7d58677d38a39422c272a30ba4b6da2a ;; \
      *) echo "unsupported architecture: $(dpkg --print-architecture)" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/binaryen.tar.gz \
      "https://github.com/WebAssembly/binaryen/releases/download/version_${BINARYEN_VERSION}/binaryen-version_${BINARYEN_VERSION}-${BINARYEN_ARCH}-linux.tar.gz"; \
    echo "${BINARYEN_SHA256}  /tmp/binaryen.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/binaryen.tar.gz -C /tmp --strip-components=2 "binaryen-version_${BINARYEN_VERSION}/bin/wasm-opt"; \
    install -m 755 /tmp/wasm-opt /usr/local/bin/wasm-opt; \
    rm -rf /tmp/binaryen.tar.gz /tmp/wasm-opt; \
    wasm-opt --version

RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-pack --version 0.15.0 --locked

COPY engine/Cargo.toml engine/Cargo.lock ./
COPY engine/src/ src/
COPY engine/shaders/ shaders/

# «Своя копия luars»: `engine/Cargo.toml` зависит от неё по пути `../luars` — то есть, при
# `WORKDIR /app/engine`, от `/app/luars`.
COPY luars/Cargo.toml /app/luars/Cargo.toml
COPY luars/src/ /app/luars/src/

# «Своя копия cosmic-text»: `[patch.crates-io]` в `engine/Cargo.toml` берёт её по пути
# `../cosmic-text`, то есть `/app/cosmic-text`, — `glyphon` раскладывает текст ею, а не версией с
# crates.io.
COPY cosmic-text/Cargo.toml /app/cosmic-text/Cargo.toml
COPY cosmic-text/src/ /app/cosmic-text/src/

RUN wasm-pack build --target web --release -- --locked

# --- Stage 2: собрать службу сжатия картинок моделей (importer/) ---
# `importer/` зависит от `engine/` по пути `../engine` (тот — от `../luars`), поэтому сюда же копируются
# исходники движка и luars. `[patch.crates-io]` из `engine/Cargo.toml` для зависимости не действует —
# `cosmic-text` службе не нужен (он нужен только wasm-сборке).
FROM rust:1-bookworm AS importer-builder
WORKDIR /app/importer

COPY importer/Cargo.toml importer/Cargo.lock ./
COPY importer/src/ src/

COPY engine/Cargo.toml /app/engine/Cargo.toml
COPY engine/src/ /app/engine/src/
COPY engine/shaders/ /app/engine/shaders/
COPY luars/Cargo.toml /app/luars/Cargo.toml
COPY luars/src/ /app/luars/src/

RUN cargo build --release --locked

# --- Stage 3: служба сжатия (docker compose, служба importer) ---
# Не последняя стадия: `docker build .` по-прежнему собирает nginx со страницей, службу compose
# берёт по `target: importer`.
FROM debian:bookworm-slim AS importer
COPY --from=importer-builder /app/importer/target/release/importer /usr/local/bin/importer
CMD ["importer", "watch", "/games"]

# --- Stage 4: собрать страницу (Vite) ---
FROM node:22-bookworm AS web-builder
WORKDIR /app

COPY --from=wasm-builder /app/engine/pkg ./engine/pkg
COPY games/ ./games/

# Публикуемый образ несёт готовые `games/*/imported/` (Фаза 51, требование 14): проход службы по своей
# копии `games/` до сборки страницы — vite копирует games/ в dist уже с `imported/`. Папка
# `imported/` с рабочей копии в образ тоже попадает, но проход сверяет хеши и дожимает только нужное.
COPY --from=importer-builder /app/importer/target/release/importer /usr/local/bin/importer
RUN importer once /app/games

WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci

COPY web/ ./
RUN npm run build

# --- Stage 5: раздать статику ---
FROM nginx:1.27-alpine
COPY deploy/nginx.conf /etc/nginx/nginx.conf
COPY --from=web-builder /app/web/dist /usr/share/nginx/html

EXPOSE 80
