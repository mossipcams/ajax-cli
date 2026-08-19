#!/usr/bin/env node
'use strict';
const fs = require('fs');
const path = require('path');
const readline = require('readline');
const loadFail = process.argv.includes('--load-fail');
const holdPromptMode = process.argv.includes('--hold-prompt');
const malformedMode = process.argv.includes('--malformed');
const badInitialize = process.argv.includes('--bad-initialize');
const permissionMode = process.argv.includes('--permission');
const resumeMode = process.argv.includes('--resume') || process.argv.includes('--resume-fail');
const resumeFail = process.argv.includes('--resume-fail');
const protocolVersion = process.argv.includes('--protocol-v2') ? 2 : 1;
const cursorModels = process.argv.includes('--cursor-models');
const cursorLiveModels = process.argv.includes('--cursor-live-models');
const acceptUnadvertisedGrokHigh = process.argv.includes('--accept-unadvertised-grok-high');
const ignoreSpawnModelOnce = process.argv.includes('--ignore-spawn-model-once');
const refuseInBandOnce = process.argv.includes('--refuse-in-band-once');
const modelRefuse = process.argv.includes('--model-refuse');
const sessionId = 'fake-sess-1';
const cliDefaultModel = process.argv.includes('--cli-default-model')
  ? 'composer-2.5[fast=true]'
  : null;

function spawnGeneration() {
  const counterPath = path.join(process.cwd(), '.fake-acp-spawn-gen');
  let gen = 0;
  try {
    gen = parseInt(fs.readFileSync(counterPath, 'utf8'), 10) || 0;
  } catch {}
  gen += 1;
  fs.writeFileSync(counterPath, String(gen));
  return gen;
}

const spawnGen = spawnGeneration();
const firstSpawnAttempt = spawnGen === 1;

function spawnModelFromArgv() {
  if (ignoreSpawnModelOnce && firstSpawnAttempt) {
    return null;
  }
  // #952: --model-refuse simulates a harness that keeps its own default and
  // rejects operator pins at spawn as well as in-band.
  if (modelRefuse) {
    return null;
  }
  const idx = process.argv.indexOf('--model');
  if (idx >= 0 && idx + 1 < process.argv.length) {
    const model = process.argv[idx + 1];
    // Live Cursor accepts cursor-grok catalog ids on spawn argv; other cursor-
    // prefixed catalog ids are ignored until in-band apply.
    if (model.startsWith('cursor-grok-')) {
      return model;
    }
    if (model.startsWith('cursor-')) {
      return null;
    }
    return model;
  }
  return null;
}
let currentModel = spawnModelFromArgv() ?? cliDefaultModel ?? 'harness-default';
let heldPromptId = null;
let holdRemaining = holdPromptMode ? 1 : 0;

function modelConfigOptions() {
  const options = [
    { value: 'harness-default', name: 'Harness default' },
    { value: 'composer-2.5', name: 'Composer 2.5' },
    { value: 'gpt-5.6-sol[medium]', name: 'GPT-5.6-Sol (medium)' },
  ];
  if (cursorModels || cursorLiveModels) {
    options.push(
      { value: 'composer-2.5[fast=true]', name: 'Composer Fast' },
      { value: 'gpt-5.6-sol[effort=high,fast=false]', name: 'GPT-5.6-Sol High' },
    );
    if (cursorLiveModels) {
      options.push({ value: 'grok-4.6[effort=high,fast=true]', name: 'Grok High Fast' });
    } else {
      options.push(
        { value: 'grok-4.6[effort=high,fast=false]', name: 'Grok High' },
        { value: 'grok-4.6[effort=high,fast=true]', name: 'Grok High Fast' },
      );
    }
  }
  return [{
    id: 'model',
    name: 'Model',
    type: 'select',
    currentValue: currentModel,
    options,
  }];
}

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
    if (badInitialize) {
      process.stderr.write('agent login required\n');
      send({ jsonrpc: '2.0', id, result: { protocolVersion: 'broken' } });
      return;
    }
    send({
      jsonrpc: '2.0',
      id,
      result: {
        protocolVersion,
        agentCapabilities: {
          loadSession: true,
          sessionCapabilities: resumeMode ? { resume: {} } : {},
        },
      },
    });
    return;
  }
  if (method === 'session/new') {
    send({
      jsonrpc: '2.0',
      id,
      result: {
        sessionId,
        configOptions: modelConfigOptions(),
      },
    });
    if (malformedMode) process.stdout.write('{not-json}\n');
    return;
  }
  if (method === 'session/load' || method === 'session/resume') {
    if (method === 'session/load' && loadFail) {
      send({
        jsonrpc: '2.0',
        id,
        error: { code: -32000, message: 'load failed' },
      });
      return;
    }
    if (method === 'session/resume' && resumeFail) {
      send({ jsonrpc: '2.0', id, error: { code: -32000, message: 'resume failed' } });
      return;
    }
    replayUpdate('replayed');
    send({
      jsonrpc: '2.0',
      id,
      result: { configOptions: modelConfigOptions() },
    });
    return;
  }
  // Model selection: echo what the client asked for so tests can assert the
  // request shape each harness family uses.
  if (method === 'session/set_model' || method === 'session/set_config_option') {
    const requested = params?.modelId ?? params?.value ?? '';
    const unadvertisedGrokHigh =
      acceptUnadvertisedGrokHigh &&
      requested === 'grok-4.6[effort=high,fast=false]';
    const refuseInBand =
      requested &&
      requested !== currentModel &&
      !unadvertisedGrokHigh &&
      (modelRefuse || (refuseInBandOnce && firstSpawnAttempt));
    if (refuseInBand) {
      send({
        jsonrpc: '2.0',
        id,
        error: { code: -32000, message: 'model refused' },
      });
      return;
    }
    if (params?.configId === 'model' || method === 'session/set_model') {
      currentModel = requested || currentModel;
    }
    send({
      jsonrpc: '2.0',
      id,
      result: { configOptions: modelConfigOptions() },
    });
    replayUpdate(`model:${method}:${requested}`);
    return;
  }
  if (method === 'session/prompt') {
    if (permissionMode) {
      heldPromptId = id;
      send({
        jsonrpc: '2.0',
        id: 42,
        method: 'session/request_permission',
        params: {
          sessionId,
          toolCall: { toolCallId: 'call-1', title: 'Run tests' },
          options: [
            { optionId: 'allow-once', name: 'Allow once', kind: 'allow_once' },
            { optionId: 'reject-once', name: 'Reject once', kind: 'reject_once' },
          ],
        },
      });
      return;
    }
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
    // Every installed harness rejects cancel as a request; it is a
    // notification. Answer like they do so the client cannot regress.
    send({
      jsonrpc: '2.0',
      id,
      error: { code: -32601, message: '"Method not found": session/cancel' },
    });
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
    // ACP cancellation is a notification: it ends the in-flight turn with
    // stopReason "cancelled".
    if (msg.method === 'session/cancel' && heldPromptId !== null) {
      send({
        jsonrpc: '2.0',
        id: heldPromptId,
        result: { stopReason: 'cancelled' },
      });
      heldPromptId = null;
    }
    if (msg.method !== 'session/cancel') replayUpdate(`notification:${msg.method}`);
    return;
  }
  if (permissionMode && msg.id === 42 && msg.result) {
    const outcome = msg.result.outcome ?? {};
    replayUpdate(`permission:${outcome.outcome}:${outcome.optionId ?? ''}`);
    send({ jsonrpc: '2.0', id: heldPromptId, result: { stopReason: 'end_turn' } });
    heldPromptId = null;
    return;
  }
  if (msg.id !== undefined && msg.method) {
    handleRequest(msg);
  }
});
process.stdin.on('end', () => process.exit(0));
