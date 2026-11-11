ARG BASE
FROM ${BASE}

USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends nodejs npm \
    && rm -rf /var/lib/apt/lists/*

USER agent
RUN npm install --global --prefix /home/agent/.local @openai/codex

ENV CODEX_HOME=/state
