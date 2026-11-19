ARG BASE
FROM ${BASE}

# Clients for the project's services (sai services): the agent inspects them without
# installing drivers. In the agent image, so adding them did not rebuild the base image
USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends mariadb-client redis-tools \
    && rm -rf /var/lib/apt/lists/*
USER agent

RUN curl -fsSL https://claude.ai/install.sh | bash

ENV CLAUDE_CONFIG_DIR=/state \
    DISABLE_AUTOUPDATER=1
