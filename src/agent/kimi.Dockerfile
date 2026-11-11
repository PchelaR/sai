ARG BASE
FROM ${BASE}

RUN curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash

ENV PATH=/home/agent/.kimi-code/bin:$PATH \
    KIMI_CODE_HOME=/state \
    KIMI_CODE_NO_AUTO_UPDATE=1
