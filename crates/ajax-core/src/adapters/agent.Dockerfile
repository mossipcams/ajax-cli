FROM mcr.microsoft.com/playwright:v1.62.1-noble

ARG HOST_UID=501
ARG HOST_GID=20
ARG HOST_HOME=/Users/matt

ENV DEBIAN_FRONTEND=noninteractive
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      git openssh-client curl ca-certificates python3 build-essential pkg-config \
      procps ripgrep jq tmux socat libsqlite3-dev \
 && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg -o /usr/share/keyrings/githubcli-archive-keyring.gpg \
 && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" > /etc/apt/sources.list.d/github-cli.list \
 && apt-get update \
 && apt-get install -y --no-install-recommends gh \
 && rm -rf /var/lib/apt/lists/*

RUN git config --system credential.https://github.com.helper '!gh auth git-credential'

RUN npm install -g \
      @anthropic-ai/claude-code \
      @anthropic-ai/claude-agent-sdk@0.3.302 \
      @agentclientprotocol/claude-agent-acp@0.93.0 \
      @agentclientprotocol/codex-acp@2.2.2 \
      @openai/codex@0.163.0 \
      acpx@0.21.1 \
      @earendil-works/pi-coding-agent@1.3.0 \
      @cursor/sdk@1.0.37 \
      opencode-ai@1.19.5
RUN mkdir -p /opt/ajax && ln -s "$(npm root -g)/@anthropic-ai/claude-agent-sdk" /opt/ajax/claude-agent-sdk \
 && ln -s "$(npm root -g)/@cursor/sdk" /opt/ajax/cursor-sdk

RUN printf '%s\n' \
      '#!/bin/sh' \
      'if [ -n "$AJAX_SSH_AGENT_RELAY" ]; then' \
      '  sock=/tmp/ajax-ssh-agent.sock' \
      '  rm -f "$sock"' \
      '  socat UNIX-LISTEN:"$sock",fork,mode=600 TCP:"$AJAX_SSH_AGENT_RELAY" </dev/null >/dev/null 2>&1 &' \
      '  i=0' \
      '  while [ ! -S "$sock" ] && [ "$i" -lt 100 ]; do sleep 0.02; i=$((i+1)); done' \
      '  export SSH_AUTH_SOCK="$sock"' \
      'fi' \
      'exec "$@"' > /opt/ajax/agent-entry \
 && chmod 755 /opt/ajax/agent-entry

COPY . /tmp/ctx
RUN if [ -d /tmp/ctx/pi-acp ]; then \
      R="$(npm root -g)" \
      && cp -a /tmp/ctx/pi-acp "$R/pi-acp" \
      && ln -s "$R/pi-acp/dist/index.js" /usr/local/bin/pi-acp; \
    fi \
 && rm -rf /tmp/ctx

RUN mkdir -p /opt/cargo/bin /opt/cursor \
 && curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none --no-modify-path \
 && case "$(dpkg --print-architecture)" in arm64) NEXTEST=linux-arm ;; *) NEXTEST=linux ;; esac \
 && curl -fsSL "https://get.nexte.st/latest/$NEXTEST" | tar zxf - -C /opt/cargo/bin

RUN curl -fsSL https://cursor.com/install | HOME=/opt/cursor bash \
 && ln -s /opt/cursor/.local/bin/cursor-agent /usr/local/bin/cursor-agent

RUN (getent passwd "$HOST_UID" >/dev/null || useradd -o -u "$HOST_UID" -g "$HOST_GID" -M -d "$HOST_HOME" -s /bin/bash agent) \
 && usermod -d "$HOST_HOME" "$(getent passwd "$HOST_UID" | cut -d: -f1)" \
 && mkdir -p "$HOST_HOME/.claude" "$HOST_HOME/.codex" "$HOST_HOME/.agents" "$HOST_HOME/.cursor" \
             "$HOST_HOME/.config" "$HOST_HOME/.ajax-router" "$HOST_HOME/Projects" \
             "$HOST_HOME/.local/state" "$HOST_HOME/.local/share" "$HOST_HOME/.pi" \
 && mkdir -p /opt/cargo/registry /opt/cargo/git \
 && chown -R "$HOST_UID:$HOST_GID" "$HOST_HOME" /opt/rustup /opt/cargo
ENTRYPOINT ["/opt/ajax/agent-entry"]
