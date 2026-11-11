ARG BASE
FROM ${BASE}

RUN printf '%s\n' 'set -g extended-keys on' 'set -sg escape-time 100' > /home/agent/.tmux.conf

RUN curl -fsSL https://claude.ai/install.sh | bash

ENV CLAUDE_CONFIG_DIR=/state \
    DISABLE_AUTOUPDATER=1
