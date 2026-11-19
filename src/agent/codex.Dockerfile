ARG BASE
FROM ${BASE}

# nodejs and npm for Codex, the clients for the project's services (sai services), rsync
# for the report of what each answer changed
USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends nodejs npm mariadb-client redis-tools rsync \
    && rm -rf /var/lib/apt/lists/*

USER agent
RUN npm install --global --prefix /home/agent/.local @openai/codex

ENV CODEX_HOME=/state
