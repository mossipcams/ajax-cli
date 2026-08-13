#!/usr/bin/env node
import readline from 'node:readline';
import process from 'node:process';
import fs from 'node:fs';
import path from 'node:path';

function argvWithWorktreeFlags() {
  const fromCli = process.argv.slice(2);
  const argsFile = path.join(process.cwd(), '.ajax-fake-acp-args');
  try {
    const extra = fs.readFileSync(argsFile, 'utf8').trim();
    if (extra) {
      return [...extra.split(/\s+/), ...fromCli];
    }
  } catch {
    // no worktree-local flags
  }
  return fromCli;
}

const argv = argvWithWorktreeFlags();
const loadFail = argv.includes('--load-fail');
const noLoadSession = argv.includes('--no-load-session');
const sessionId = 'fake-sess-1';

if (argv.includes('models') && !argv.includes('acp')) {
  process.stdout.write('Available models\n\nauto - Auto\ncomposer-2.5 - Composer 2.5\n');
  process.exit(0);
}

if (!argv.includes('acp')) {
  process.exit(1);
}

let hangPromptId = null;
let hangResolve = null;
let pendingPermPromptId = null;

function send(obj) {
  process.stdout.write(`${JSON.stringify(obj)}\n`);
}

function replayUpdate(text, sessionUpdate = 'agent_message_chunk') {
  send({
    jsonrpc: '2.0',
    method: 'session/update',
    params: {
      sessionId,
      update: {
        sessionUpdate,
        content: { type: 'text', text },
      },
    },
  });
}

function toolCallUpdate() {
  send({
    jsonrpc: '2.0',
    method: 'session/update',
    params: {
      sessionId,
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc1',
        title: 'Fake tool',
        kind: 'run',
        status: 'completed',
        locations: [],
      },
    },
  });
}

function unknownUpdate() {
  send({
    jsonrpc: '2.0',
    method: 'session/update',
    params: {
      sessionId,
      update: {
        sessionUpdate: 'totally_unknown_kind',
        foo: 'bar',
      },
    },
  });
}

function finishPrompt(id, stopReason = 'end_turn') {
  send({ jsonrpc: '2.0', id, result: { stopReason } });
}

function handleRequest(msg) {
  const { id, method, params } = msg;
  if (method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id,
      result: { agentCapabilities: { loadSession: !noLoadSession } },
    });
    return;
  }
  if (method === 'session/new') {
    if (!params || !Array.isArray(params.mcpServers)) {
      send({
        jsonrpc: '2.0',
        id,
        error: { code: -32602, message: 'mcpServers array required' },
      });
      return;
    }
    send({ jsonrpc: '2.0', id, result: { sessionId } });
    return;
  }
  if (method === 'session/load') {
    if (loadFail) {
      send({
        jsonrpc: '2.0',
        id,
        error: { code: -32000, message: 'load failed' },
      });
      return;
    }
    replayUpdate('replayed');
    send({ jsonrpc: '2.0', id, result: {} });
    return;
  }
  if (method === 'session/prompt') {
    const text =
      params?.prompt?.[0]?.text ??
      params?.prompt?.[0]?.content?.text ??
      params?.text ??
      '';
    handlePrompt(id, String(text));
    return;
  }
  if (method === 'session/cancel') {
    if (hangPromptId !== null) {
      const idToFinish = hangPromptId;
      hangPromptId = null;
      hangResolve = null;
      finishPrompt(idToFinish, 'cancelled');
    }
    send({ jsonrpc: '2.0', id, result: {} });
    return;
  }
  send({
    jsonrpc: '2.0',
    id,
    error: { code: -32601, message: `unknown method: ${method}` },
  });
}

function handlePrompt(id, text) {
  if (text === '__DIE__') {
    process.exit(1);
    return;
  }
  if (text === '__PERM__') {
    send({
      jsonrpc: '2.0',
      id: 42,
      method: 'session/request_permission',
      params: {
        requestId: '42',
        title: 'Approve?',
        detail: 'Need approval',
      },
    });
    pendingPermPromptId = id;
    return;
  }
  if (text === '__HANG__') {
    hangPromptId = id;
    hangResolve = () => {};
    replayUpdate('hung');
    return;
  }
  if (text === '__DELAY__') {
    setTimeout(() => {
      replayUpdate('delayed');
      finishPrompt(id);
    }, 250);
    return;
  }
  if (text === '__TOOL__') {
    toolCallUpdate();
    replayUpdate('tool done');
    finishPrompt(id);
    return;
  }
  if (text === '__UNKNOWN__') {
    unknownUpdate();
    finishPrompt(id);
    return;
  }
  replayUpdate(text);
  finishPrompt(id);
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });
rl.on('line', (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  let msg;
  try {
    msg = JSON.parse(trimmed);
  } catch {
    return;
  }
  if (msg.id !== undefined && msg.method) {
    handleRequest(msg);
    return;
  }
  if (msg.id !== undefined && (msg.result !== undefined || msg.error !== undefined)) {
    if (pendingPermPromptId !== null && msg.id === 42) {
      finishPrompt(pendingPermPromptId);
      pendingPermPromptId = null;
    }
  }
});
process.stdin.on('end', () => process.exit(0));
