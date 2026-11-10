ARG BASE
FROM ${BASE}

RUN curl -fsSL https://claude.ai/install.sh | bash

ENV CLAUDE_CONFIG_DIR=/state \
    DISABLE_AUTOUPDATER=1
