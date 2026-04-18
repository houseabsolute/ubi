FROM rust:latest

RUN apt-get update \
    && apt-get install -y curl git \
    && rm -rf /var/lib/apt/lists/*

RUN curl https://mise.run | sh
ENV PATH="/root/.local/share/mise/bin:/root/.local/bin:$PATH"
