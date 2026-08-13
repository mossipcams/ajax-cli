#!/usr/bin/env node
'use strict';
const readline = require('readline');
const loadFail = process.argv.includes('--load-fail');
const holdPromptMode = process.argv.includes('--hold-prompt');
const sessionId = 'fake-sess-1';
let heldPromptId = null;
let holdRemaining = holdPromptMode ? 1 : 0;

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

function replayUpdate(text) {
  send({
    jsonrpc: '2.0',
    method: 'session/update',
    params: {
      sessionId,
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text },
      },
    },
  });
}

function handleRequest(msg) {
  const { id, method, params } = msg;
  if (method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id,
      result: { agentCapabilities: { loadSession: true } },
    });
    return;
  }
  if (method === 'session/new') {
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
    if (holdRemaining > 0) {
      holdRemaining -= 1;
      heldPromptId = id;
      return;
    }
    replayUpdate('pong');
    send({ jsonrpc: '2.0', id, result: { stopReason: 'end_turn' } });
    return;
  }
  if (method === 'session/cancel') {
    if (heldPromptId !== null) {
      send({
        jsonrpc: '2.0',
        id: heldPromptId,
        result: { stopReason: 'cancelled' },
      });
      heldPromptId = null;
    }
    send({ jsonrpc: '2.0', id, result: {} });
    return;
  }
  send({
    jsonrpc: '2.0',
    id,
    error: { code: -32601, message: 'unknown method: ' + method },
  });
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
  if (msg.method && msg.id === undefined) {
    return;
  }
  if (msg.id !== undefined && msg.method) {
    handleRequest(msg);
  }
});
process.stdin.on('end', () => process.exit(0));
