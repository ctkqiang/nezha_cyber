# =============================================================================
# 哪吒网络安全 TUI —— 多阶段 Docker 构建
#
# 阶段 1 (builder): 使用 Rust 官方镜像编译 release 二进制
# 阶段 2 (runtime): 使用最小化 Debian 镜像，仅包含运行时依赖
#
# 构建命令:
#   docker build -t nezha-cyber:latest .
#
# 运行命令:
#   docker run -it -e DEEPSEEK_TOKEN=sk-xxx nezha-cyber:latest
# =============================================================================

# ---- 阶段 1: 编译构建 ----
FROM rust:1-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

COPY src/ ./src/

RUN cargo build --release

# ---- 阶段 2: 运行时 ----
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 1000 nezha \
    && useradd --system --uid 1000 --gid nezha --create-home --shell /sbin/nologin nezha

COPY --from=builder /app/target/release/nezha_cyber /usr/local/bin/nezha_cyber

RUN chown nezha:nezha /usr/local/bin/nezha_cyber

USER nezha
WORKDIR /home/nezha

ENV DEEPSEEK_TOKEN=""

ENTRYPOINT ["nezha_cyber"]
