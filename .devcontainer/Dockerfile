FROM rust:latest

RUN apt-get update \
    && apt-get install -y curl git \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustfmt clippy

ARG MISE_VERSION=v2026.4.28
ARG MISE_SHA256=9655492db554e8f70a69830f54307ac0f4681d6c42f9844e862528b7853d09d1
RUN curl -fsSL "https://github.com/jdx/mise/releases/download/${MISE_VERSION}/mise-${MISE_VERSION}-linux-x64" \
        -o /usr/local/bin/mise \
    && echo "${MISE_SHA256}  /usr/local/bin/mise" | sha256sum -c \
    && chmod +x /usr/local/bin/mise

RUN useradd -m -s /bin/bash dev \
    && mkdir -p /workspace \
    && chown dev:dev /workspace

USER dev

RUN echo 'eval "$(mise activate bash)"' >> /home/dev/.bashrc
