#!/usr/bin/env node
'use strict';
const fs = require('fs');
const os = require('os');
const path = require('path');
const readline = require('readline');
const loadFail = process.argv.includes('--load-fail');
const noLoadSession = process.argv.includes('--no-load-session');
const recordMethods = process.argv.includes('--record-methods');
const hangResume = process.argv.includes('--hang-resume');
const hangLoad = process.argv.includes('--hang-load');
const holdPromptMode = process.argv.includes('--hold-prompt');
const malformedMode = process.argv.includes('--malformed');
const badInitialize = process.argv.includes('--bad-initialize');
const permissionMode = process.argv.includes('--permission');
const permissionRejectOnly = process.argv.includes('--permission-reject-only');
const permissionAllowAlways = process.argv.includes('--permission-allow-always');
const permissionHold = process.argv.includes('--permission-hold');
const resumeMode = process.argv.includes('--resume') || process.argv.includes('--resume-fail');
const resumeFail = process.argv.includes('--resume-fail');
const protocolVersion = process.argv.includes('--protocol-v2') ? 2 : 1;
const cursorModels = process.argv.includes('--cursor-models');
const cursorLiveModels = process.argv.includes('--cursor-live-models');
const cursorParameterizedModels = process.argv.includes('--cursor-parameterized-models');
const cursorMode = process.argv.includes('--cursor-mode');
const acceptUnadvertisedGrokHigh = process.argv.includes('--accept-unadvertised-grok-high');
const ignoreSpawnModelOnce = process.argv.includes('--ignore-spawn-model-once');
const refuseInBandOnce = process.argv.includes('--refuse-in-band-once');
const exclusiveSessionNew = process.argv.includes('--exclusive-session-new');
const modelRefuse = process.argv.includes('--model-refuse');
const slashCommands = process.argv.includes('--slash-commands');
const slashCommandsReplace = process.argv.includes('--slash-commands-replace');
const sessionInfo = process.argv.includes('--session-info');
const sessionInfoReplace = process.argv.includes('--session-info-replace');
const elicitationForm = process.argv.includes('--elicitation-form');
const elicitationUrl = process.argv.includes('--elicitation-url');
const promptCapabilities = process.argv.includes('--prompt-capabilities');
const richOutput = process.argv.includes('--rich-output');
const sessionClose = process.argv.includes('--session-close');
const sessionCloseFail = process.argv.includes('--session-close-fail');
const rememberContext = process.argv.includes('--remember-context');
const thoughtOnly = process.argv.includes('--thought-only');
const toolOnly = process.argv.includes('--tool-only');
const noAgentText = process.argv.includes('--no-agent-text');
const promptFail = process.argv.includes('--prompt-fail');
const sessionId = 'fake-sess-1';
// ponytail: isolate fixture sidecar files under FAKE_ACP_STATE_DIR or a per-pid tmp dir
// so verification does not litter the crate root when cwd is not the worktree.
const configuredStateRoot = process.env.FAKE_ACP_STATE_DIR;
const stateRoot = configuredStateRoot || path.join(os.tmpdir(), `fake-acp-${process.pid}`);
fs.mkdirSync(stateRoot, { recursive: true });
if (!configuredStateRoot) {
  process.on('exit', () => fs.rmSync(stateRoot, { recursive: true, force: true }));
}
const persistedSessionsPath = path.join(stateRoot, '.fake-acp-sessions');
const methodsLogPath = path.join(stateRoot, '.fake-acp-methods');
const contextMemoryPath = path.join(stateRoot, '.fake-acp-context-memory');

function clearContextMemory() {
  if (!rememberContext) return;
  try {
    fs.unlinkSync(contextMemoryPath);
  } catch {}
}

function loadContextMemory() {
  if (!rememberContext) return null;
  try {
    return fs.readFileSync(contextMemoryPath, 'utf8');
  } catch {
    return null;
  }
}

function saveContextMemory(value) {
  if (!rememberContext) return;
  fs.writeFileSync(contextMemoryPath, value);
}

function promptTextFromParams(params) {
  if (!Array.isArray(params?.prompt)) return '';
  return params.prompt
    .filter((block) => block?.type === 'text')
    .map((block) => block.text || '')
    .join('');
}

function recordMethod(method) {
  if (!recordMethods) return;
  let methods = [];
  try {
    methods = JSON.parse(fs.readFileSync(methodsLogPath, 'utf8'));
  } catch {}
  methods.push(method);
  fs.writeFileSync(methodsLogPath, JSON.stringify(methods));
}

function loadPersistedSessions() {
  try {
    const parsed = JSON.parse(fs.readFileSync(persistedSessionsPath, 'utf8'));
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function savePersistedSessions(ids) {
  fs.writeFileSync(persistedSessionsPath, JSON.stringify(ids));
}

function persistSession(id) {
  const ids = loadPersistedSessions();
  if (!ids.includes(id)) ids.push(id);
  savePersistedSessions(ids);
}

function forgetSession(id) {
  savePersistedSessions(loadPersistedSessions().filter((known) => known !== id));
}

function sessionKnown(id) {
  return (!configuredStateRoot && id === sessionId) || loadPersistedSessions().includes(id);
}
const cliDefaultModel = process.argv.includes('--cli-default-model')
  ? (cursorParameterizedModels ? 'composer-2.5' : 'composer-2.5[fast=true]')
  : null;

function spawnGeneration() {
  const counterPath = path.join(stateRoot, '.fake-acp-spawn-gen');
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
const exclusiveLockPath = path.join(stateRoot, '.fake-acp-exclusive-lock');

function pidAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return err.code === 'EPERM';
  }
}

function holdExclusiveLock() {
  if (!exclusiveSessionNew) return;
  fs.writeFileSync(exclusiveLockPath, String(process.pid));
}

function releaseExclusiveLock() {
  if (!exclusiveSessionNew) return;
  try {
    const pid = parseInt(fs.readFileSync(exclusiveLockPath, 'utf8'), 10);
    if (pid === process.pid) fs.unlinkSync(exclusiveLockPath);
  } catch {}
}

function assertExclusiveSessionNew() {
  if (!exclusiveSessionNew) return;
  try {
    const pid = parseInt(fs.readFileSync(exclusiveLockPath, 'utf8'), 10);
    if (Number.isFinite(pid) && pid !== process.pid && pidAlive(pid)) {
      // Live Cursor rejects a second ACP child with transport close during session/new.
      process.exit(1);
    }
  } catch {}
  holdExclusiveLock();
}

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
let currentEffort = 'high';
let currentFast = cursorParameterizedModels ? true : null;
let currentMode = 'default';
let heldPromptId = null;
let heldElicitationId = null;
let holdRemaining = holdPromptMode ? 1 : 0;

function modelConfigOptions() {
  if (cursorParameterizedModels) {
    return [
      {
        id: 'model',
        name: 'Model',
        category: 'model',
        type: 'select',
        currentValue: currentModel,
        options: [
          { value: 'composer-2.5', name: 'Composer 2.5' },
          { value: 'grok-4.6', name: 'Grok 4.6' },
          { value: 'gpt-5.6-sol', name: 'GPT-5.6-Sol' },
        ],
      },
      {
        id: 'effort',
        name: 'Effort',
        category: 'thought_level',
        type: 'select',
        currentValue: currentEffort,
        options: [
          { value: 'high', name: 'High' },
          { value: 'medium', name: 'Medium' },
          { value: 'low', name: 'Low' },
        ],
      },
      {
        id: 'fast',
        name: 'Fast',
        category: 'model_config',
        type: 'boolean',
        currentValue: currentFast,
      },
    ];
  }
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
  const configOptions = [{
    id: 'model',
    name: 'Model',
    type: 'select',
    currentValue: currentModel,
    options,
  }];
  if (cursorMode) {
    configOptions.push({
      id: 'mode',
      name: 'Mode',
      category: 'mode',
      type: 'select',
      currentValue: currentMode,
      options: [
        { value: 'default', name: 'Default' },
        { value: 'agent', name: 'Agent' },
      ],
    });
  }
  return configOptions;
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

function replayRichOutput() {
  send({
    jsonrpc: '2.0',
    method: 'session/update',
    params: {
      sessionId,
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: {
          type: 'image',
          mimeType: 'image/png',
          uri: 'https://example.com/shot.png',
        },
      },
    },
  });
  send({
    jsonrpc: '2.0',
    method: 'session/update',
    params: {
      sessionId,
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'call-rich',
        title: 'Attach screenshot',
        kind: 'other',
        status: 'completed',
        content: [
          {
            type: 'content',
            content: {
              type: 'resource_link',
              name: 'README.md',
              uri: 'file:///README.md',
            },
          },
          { type: 'terminal', terminalId: 'term-ignored' },
        ],
      },
    },
  });
}

function sendAvailableCommands(commands) {
  send({
    jsonrpc: '2.0',
    method: 'session/update',
    params: {
      sessionId,
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: commands,
      },
    },
  });
}

function sendSessionInfo(title) {
  send({
    jsonrpc: '2.0',
    method: 'session/update',
    params: {
      sessionId,
      update: {
        sessionUpdate: 'session_info_update',
        title,
      },
    },
  });
}

function defaultSlashCommands() {
  return [
    {
      name: 'web',
      description: 'Query the web',
      input: { hint: 'query' },
    },
    {
      name: 'help',
      description: 'Show help',
    },
  ];
}

function replacementSlashCommands() {
  return [
    {
      name: 'plan',
      description: 'Create a plan',
    },
  ];
}

function handleRequest(msg) {
  const { id, method, params } = msg;
  recordMethod(method);
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
          loadSession: !noLoadSession,
          sessionCapabilities: {
            ...(resumeMode ? { resume: {} } : {}),
            ...(sessionClose ? { close: {} } : {}),
          },
          ...(promptCapabilities
            ? { promptCapabilities: { image: true, embeddedContext: true } }
            : {}),
        },
      },
    });
    return;
  }
  if (method === 'session/new') {
    assertExclusiveSessionNew();
    clearContextMemory();
    persistSession(sessionId);
    send({
      jsonrpc: '2.0',
      id,
      result: {
        sessionId,
        configOptions: modelConfigOptions(),
      },
    });
    if (slashCommands) sendAvailableCommands(defaultSlashCommands());
    if (sessionInfo) sendSessionInfo('Initial session title');
    if (malformedMode) process.stdout.write('{not-json}\n');
    return;
  }
  if (method === 'session/close') {
    const closedId = params?.sessionId ?? sessionId;
    fs.writeFileSync(
      path.join(stateRoot, '.fake-acp-session-close-called'),
      closedId,
    );
    forgetSession(closedId);
    if (sessionCloseFail) {
      send({
        jsonrpc: '2.0',
        id,
        error: { code: -32000, message: 'close failed' },
      });
      return;
    }
    send({ jsonrpc: '2.0', id, result: {} });
    return;
  }
  if (method === 'session/load' || method === 'session/resume') {
    if (method === 'session/resume' && hangResume) {
      return;
    }
    if (method === 'session/load' && hangLoad) {
      return;
    }
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
    const requestedId = params?.sessionId ?? sessionId;
    if (!sessionKnown(requestedId)) {
      send({
        jsonrpc: '2.0',
        id,
        error: { code: -32000, message: 'session not found' },
      });
      return;
    }
    persistSession(requestedId);
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
    const configId = params?.configId;
    const rawValue = params?.value ?? params?.modelId;
    const selectValue =
      typeof rawValue === 'object' && rawValue !== null && rawValue.type === 'boolean'
        ? String(rawValue.value)
        : typeof rawValue === 'boolean'
          ? String(rawValue)
          : rawValue ?? '';
    const refuseInBand =
      selectValue &&
      configId === 'model' &&
      selectValue !== currentModel &&
      (modelRefuse || (refuseInBandOnce && firstSpawnAttempt));
    if (refuseInBand) {
      send({
        jsonrpc: '2.0',
        id,
        error: { code: -32000, message: 'model refused' },
      });
      return;
    }
    if (configId === 'model' || method === 'session/set_model') {
      currentModel = (typeof rawValue === 'string' ? rawValue : selectValue) || currentModel;
    } else if (configId === 'effort') {
      currentEffort = selectValue || currentEffort;
    } else if (configId === 'fast') {
      if (typeof rawValue === 'object' && rawValue !== null && rawValue.type === 'boolean') {
        currentFast = rawValue.value;
      } else if (typeof rawValue === 'boolean') {
        currentFast = rawValue;
      } else {
        currentFast = selectValue === 'true';
      }
    } else if (configId === 'mode') {
      currentMode = selectValue || currentMode;
    }
    send({
      jsonrpc: '2.0',
      id,
      result: { configOptions: modelConfigOptions() },
    });
    replayUpdate(
      `model:${method}:${
        configId === 'model' || method === 'session/set_model'
          ? (typeof rawValue === 'string' ? rawValue : selectValue)
          : `${configId}:${selectValue}`
      }`,
    );
    return;
  }
  if (method === 'session/prompt') {
    if (slashCommandsReplace) {
      sendAvailableCommands(replacementSlashCommands());
    }
    if (sessionInfoReplace) {
      sendSessionInfo('Renamed session');
    }
    if (elicitationForm) {
      heldPromptId = id;
      heldElicitationId = 77;
      send({
        jsonrpc: '2.0',
        id: heldElicitationId,
        method: 'elicitation/create',
        params: {
          mode: 'form',
          sessionId,
          message: 'Pick deployment target',
          requestedSchema: {
            type: 'object',
            properties: {
              target: {
                type: 'string',
                title: 'Target',
                enum: ['staging', 'production'],
              },
              confirmed: {
                type: 'boolean',
                title: 'Confirmed',
              },
              replicas: {
                type: 'number',
                title: 'Replicas',
                minimum: 1,
                maximum: 5,
              },
            },
            required: ['target'],
          },
        },
      });
      return;
    }
    if (elicitationUrl) {
      heldPromptId = id;
      heldElicitationId = 78;
      send({
        jsonrpc: '2.0',
        id: heldElicitationId,
        method: 'elicitation/create',
        params: {
          mode: 'url',
          sessionId,
          message: 'Open the link',
          elicitationId: 'elicit-url-1',
          url: 'https://example.com/oauth',
        },
      });
      return;
    }
    if (permissionMode || permissionRejectOnly || permissionAllowAlways) {
      heldPromptId = id;
      const options = permissionRejectOnly
        ? [{ optionId: 'reject-once', name: 'Reject once', kind: 'reject_once' }]
        : permissionAllowAlways
          ? [
              { optionId: 'allow-always', name: 'Allow always', kind: 'allow_always' },
              { optionId: 'allow-once', name: 'Allow once', kind: 'allow_once' },
            ]
          : [
              { optionId: 'allow-once', name: 'Allow once', kind: 'allow_once' },
              { optionId: 'reject-once', name: 'Reject once', kind: 'reject_once' },
            ];
      send({
        jsonrpc: '2.0',
        id: 42,
        method: 'session/request_permission',
        params: {
          sessionId,
          toolCall: { toolCallId: 'call-1', title: 'Run tests' },
          options,
        },
      });
      return;
    }
    if (holdRemaining > 0) {
      holdRemaining -= 1;
      heldPromptId = id;
      return;
    }
    if (promptFail) {
      send({
        jsonrpc: '2.0',
        id,
        error: { code: -32000, message: 'prompt failed' },
      });
      return;
    }
    if (rememberContext) {
      const text = promptTextFromParams(params);
      if (text.startsWith('remember:')) {
        const value = text.slice('remember:'.length);
        saveContextMemory(value);
        replayUpdate(`stored:${value}`);
        send({ jsonrpc: '2.0', id, result: { stopReason: 'end_turn' } });
        return;
      }
      if (text === 'recall') {
        const stored = loadContextMemory();
        replayUpdate(stored ? `recalled:${stored}` : 'recalled:none');
        send({ jsonrpc: '2.0', id, result: { stopReason: 'end_turn' } });
        return;
      }
    }
    if (thoughtOnly) {
      send({
        jsonrpc: '2.0',
        method: 'session/update',
        params: {
          sessionId,
          update: {
            sessionUpdate: 'agent_thought_chunk',
            content: { type: 'text', text: 'thinking-only' },
          },
        },
      });
      send({ jsonrpc: '2.0', id, result: { stopReason: 'end_turn' } });
      return;
    }
    if (toolOnly) {
      send({
        jsonrpc: '2.0',
        method: 'session/update',
        params: {
          sessionId,
          update: {
            sessionUpdate: 'tool_call',
            toolCallId: 'call-only',
            title: 'List files',
            kind: 'other',
            status: 'completed',
          },
        },
      });
      send({ jsonrpc: '2.0', id, result: { stopReason: 'end_turn' } });
      return;
    }
    if (noAgentText) {
      send({ jsonrpc: '2.0', id, result: { stopReason: 'end_turn' } });
      return;
    }
    const kinds = Array.isArray(params?.prompt)
      ? params.prompt.map((block) => block?.type || 'unknown').join(',')
      : 'text';
    if (richOutput) {
      replayRichOutput();
    } else {
      replayUpdate(kinds === 'text' ? 'pong' : `prompt:${kinds}`);
    }
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
  if (heldElicitationId !== null && msg.id === heldElicitationId && msg.error) {
    replayUpdate(`elicitation:error:${msg.error.code ?? 'unknown'}`);
    send({ jsonrpc: '2.0', id: heldPromptId, result: { stopReason: 'end_turn' } });
    heldPromptId = null;
    heldElicitationId = null;
    return;
  }
  if (heldElicitationId !== null && msg.id === heldElicitationId && msg.result) {
    const action = msg.result.action ?? 'unknown';
    replayUpdate(`elicitation:${action}`);
    send({ jsonrpc: '2.0', id: heldPromptId, result: { stopReason: 'end_turn' } });
    heldPromptId = null;
    heldElicitationId = null;
    return;
  }
  if ((permissionMode || permissionRejectOnly || permissionAllowAlways) && msg.id === 42 && msg.result) {
    const outcome = msg.result.outcome ?? {};
    replayUpdate(`permission:${outcome.outcome}:${outcome.optionId ?? ''}`);
    if (permissionHold) {
      return;
    }
    send({ jsonrpc: '2.0', id: heldPromptId, result: { stopReason: 'end_turn' } });
    heldPromptId = null;
    return;
  }
  if (msg.id !== undefined && msg.method) {
    handleRequest(msg);
  }
});
process.stdin.on('end', () => {
  releaseExclusiveLock();
  process.exit(0);
});
process.on('exit', releaseExclusiveLock);
