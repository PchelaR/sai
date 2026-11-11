ARG BASE
FROM ${BASE}

RUN printf '%s\n' 'set -g extended-keys on' 'set -sg escape-time 100' > /home/agent/.tmux.conf

RUN curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash

ENV PATH=/home/agent/.kimi-code/bin:$PATH \
    KIMI_CODE_HOME=/state \
    KIMI_CODE_NO_AUTO_UPDATE=1
