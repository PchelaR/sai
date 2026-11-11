ARG BASE
FROM ${BASE}

RUN printf '%s\n' 'set -g extended-keys on' 'set -sg escape-time 100' > /home/agent/.tmux.conf

USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends nodejs npm \
    && rm -rf /var/lib/apt/lists/*

USER agent
RUN npm install --global --prefix /home/agent/.local @openai/codex

ENV CODEX_HOME=/state
