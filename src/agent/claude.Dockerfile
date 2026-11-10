FROM debian:trixie-slim

ARG UID

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl git ripgrep tmux xz-utils \
        build-essential pkg-config libssl-dev \
        libbz2-dev libffi-dev libgdbm-dev liblzma-dev libncurses-dev \
        libreadline-dev libsqlite3-dev uuid-dev zlib1g-dev \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid "$UID" --create-home --shell /bin/bash agent \
    && install -d -o agent -g agent /state /workspace \
    && printf '#!/bin/sh\nexec claude --permission-mode acceptEdits "$@"\n' > /usr/local/bin/agent \
    && chmod 755 /usr/local/bin/agent

USER agent

ENV PYENV_ROOT=/home/agent/.pyenv
ENV PATH=$PYENV_ROOT/shims:$PYENV_ROOT/bin:/home/agent/.local/bin:/home/agent/.cargo/bin:$PATH

RUN git clone --depth 1 https://github.com/pyenv/pyenv.git "$PYENV_ROOT" \
    && latest=$(pyenv latest -k 3) \
    && pyenv install "$latest" \
    && pyenv install 3.11.2 \
    && pyenv install 3.7.17 \
    && pyenv global "$latest" 3.11.2 3.7.17

RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal -c clippy -c rustfmt

RUN curl -fsSL https://claude.ai/install.sh | bash

ENV LANG=C.UTF-8 \
    TERM=xterm-256color \
    CLAUDE_CONFIG_DIR=/state/claude \
    DISABLE_AUTOUPDATER=1

WORKDIR /workspace

CMD ["sleep", "infinity"]
