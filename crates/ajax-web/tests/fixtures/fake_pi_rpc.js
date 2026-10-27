#!/usr/bin/env node
'use strict';

const readline = require('readline');

const sessionId = 'fake-pi-rpc-session-1';
const sessionArgIndex = process.argv.indexOf('--session-id');
const ignoreSession = process.argv.includes('--ignore-session');
const overriddenSessionId =
  !ignoreSession && sessionArgIndex >= 0 && typeof process.argv[sessionArgIndex + 1] === 'string'
    ? process.argv[sessionArgIndex + 1]
    : null;

const legacySessionArgIndex = process.argv.indexOf('--session');
if (
  legacySessionArgIndex >= 0 &&
  typeof process.argv[legacySessionArgIndex + 1] === 'string'
) {
  process.stderr.write(`No session found matching '${process.argv[legacySessionArgIndex + 1]}'\n`);
  process.exit(1);
}
const sessionFile = '/tmp/fake-pi-rpc/sessions/2026-01-01T00-00-00Z_fake-pi-rpc-session-1.jsonl';
const fakeModel = {
  id: 'fake-model',
  name: 'Fake Model',
  api: 'openai-completions',
  provider: 'fake-provider',
  reasoning: true,
  input: ['text'],
  contextWindow: 131072,
  maxTokens: 8192,
};
const sharedOtherModel = { ...fakeModel, provider: 'other-provider' };

function emit(record) {
  process.stdout.write(`${JSON.stringify(record)}\n`);
}

const flags = new Set(process.argv.slice(2));

const stateFail = flags.has('--state-fail');
const noSessionId = flags.has('--no-session-id');
const silentState = flags.has('--silent-state');
const degradeOptionals = flags.has('--degrade-optionals');
const interleaveEvent = flags.has('--interleave-event');
const rejectPrompt = flags.has('--reject-prompt');
const holdRun = flags.has('--hold-run');
const uiConfirm = flags.has('--ui-confirm');
const surviveStdinClose = flags.has('--survive-stdin-eof');
const exitAfterState = flags.has('--exit-after-state');
const interleaveText = flags.has('--interleave-text');
const silentStats = flags.has('--silent-stats');
const chatterState = flags.has('--chatter-state');
const sharedModelIds = flags.has('--shared-model-ids');

if (flags.has('--emit-u2028')) {
  process.stdout.write('{"type":"note","text":"one\u2028two"}\n');
}
if (flags.has('--emit-garbage')) {
  process.stdout.write('this line is not json\n');
}
if (flags.has('--emit-stderr')) {
  process.stderr.write('fake pi rpc stderr noise\n');
}
if (flags.has('--emit-stderr-noise')) {
  process.stderr.write('STDERR-HEAD-BEGIN ' + 'x'.repeat(8 * 1024) + ' STDERR-TAIL-END\n');
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });

let exiting = false;

rl.on('line', (raw) => {
  if (exiting) return;
  const text = raw.replace(/\r$/, '');
  let command;
  try {
    command = JSON.parse(text);
  } catch {
    return;
  }
  if (!command || typeof command !== 'object') return;

  switch (command.type) {
    case 'get_state':
      if (chatterState) {
        const chatterUntil = Date.now() + 15_000;
        let seq = 0;
        const chatter = () => {
          if (Date.now() > chatterUntil) process.exit(0);
          for (let i = 0; i < 200; i++) {
            emit({ type: 'extension_ui_request', action: 'notify', payload: { seq: seq++ } });
          }
          setImmediate(chatter);
        };
        chatter();
        break;
      }
      if (silentState) {
        break;
      }
      if (stateFail) {
        emit({
          id: command.id,
          type: 'response',
          command: 'get_state',
          success: false,
          error: 'fake pi state failure',
        });
        break;
      }
      const stateData = {
        sessionFile,
        model: sharedModelIds ? sharedOtherModel : fakeModel,
        thinkingLevel: 'medium',
        isStreaming: false,
        messageCount: 0,
      };
      if (!noSessionId) stateData.sessionId = overriddenSessionId ?? sessionId;
      emit({ id: command.id, type: 'response', command: 'get_state', success: true, data: stateData });
      if (exitAfterState) {
        exiting = true;
        process.stdout.write('', () => process.exit(1));
        break;
      }
      if (interleaveEvent) {
        emit({ type: 'extension_ui_request', action: 'menu', payload: { items: ['a'] } });
      }
      if (interleaveText) {
        emit({ type: 'message_update', assistantMessageEvent: { type: 'text_delta', delta: 'pending-text' } });
      }
      break;

    case 'get_available_models':
      if (degradeOptionals) {
        emit({ id: command.id, type: 'response', command: 'get_available_models', success: false, error: 'degraded for test' });
        break;
      }
      emit({
        id: command.id,
        type: 'response',
        command: 'get_available_models',
        success: true,
        data: {
          models: sharedModelIds
            ? [fakeModel, sharedOtherModel]
            : [fakeModel, { id: 'fake-nonreasoning', name: 'Fake Non-reasoning', api: 'anthropic-messages', provider: 'fake-provider', reasoning: false, input: ['text', 'image'], contextWindow: 262144, maxTokens: 8192 }],
        },
      });
      break;

    case 'get_available_thinking_levels':
      if (degradeOptionals) {
        emit({ id: command.id, type: 'response', command: 'get_available_thinking_levels', success: false, error: 'degraded for test' });
        break;
      }
      emit({ id: command.id, type: 'response', command: 'get_available_thinking_levels', success: true, data: { levels: ['off', 'low', 'medium', 'high'] } });
      break;

    case 'get_commands':
      if (degradeOptionals) {
        emit({ id: command.id, type: 'response', command: 'get_commands', success: false, error: 'degraded for test' });
        break;
      }
      emit({
        id: command.id,
        type: 'response',
        command: 'get_commands',
        success: true,
        data: { commands: [{ name: 'compact', description: 'Compact the conversation', source: 'builtin' }, { name: 'extensions', source: 'extension' }] },
      });
      break;

    case 'set_model':
      if (command.modelId === 'does-not-exist') {
        emit({ id: command.id, type: 'response', command: 'set_model', success: false, error: 'Model not found: does-not-exist' });
        break;
      }
      emit({ id: command.id, type: 'response', command: 'set_model', success: true, data: { id: command.modelId, provider: command.provider, name: command.modelId } });
      break;

    case 'set_thinking_level':
      emit({ id: command.id, type: 'response', command: 'set_thinking_level', success: true });
      break;

    case 'get_session_stats':
      if (silentStats) {
        break;
      }
      emit({
        id: command.id,
        type: 'response',
        command: 'get_session_stats',
        success: true,
        data: {
          sessionFile,
          sessionId,
          userMessages: 1,
          assistantMessages: 1,
          toolCalls: 0,
          toolResults: 0,
          totalMessages: 3,
          tokens: { input: 704, output: 3, cacheRead: 9655, cacheWrite: 0, total: 10362 },
          cost: 0.0035091,
          contextUsage: { tokens: 10362, contextWindow: 1000000, percent: 1.0362 },
        },
      });
      break;

    case 'prompt':
      if (rejectPrompt) {
        emit({ id: command.id, type: 'response', command: 'prompt', success: false, error: 'rejected for test' });
        break;
      }
      emit({ id: command.id, type: 'response', command: 'prompt', success: true, data: { disposition: 'started' } });
      emit({ type: 'agent_start' });
      emit({ type: 'message_update', assistantMessageEvent: { type: 'text_delta', delta: 'ok' } });
      if (Array.isArray(command.images) && command.images.length > 0) {
        const first = command.images[0];
        emit({ type: 'message_update', assistantMessageEvent: { type: 'text_delta', delta: `images=${command.images.length}:${first.type}/${first.mimeType}` } });
      }
      if (uiConfirm) {
        emit({ type: 'extension_ui_request', id: 'ui-1', method: 'confirm', title: 'Run the tool?', message: 'Allow bash?' });
        break;
      }
      if (holdRun) {
        break;
      }
      emit({ type: 'agent_end', messages: [], willRetry: false });
      emit({ type: 'agent_settled' })
      break;

    case 'compact':
      emit({ type: 'compaction_start', reason: 'manual' });
      emit({ type: 'compaction_end', reason: 'manual', aborted: false, willRetry: false, result: { tokensBefore: 100, estimatedTokensAfter: 40 } });
      emit({ id: command.id, type: 'response', command: 'compact', success: true, data: { summary: 's', tokensBefore: 100, estimatedTokensAfter: 40 } });
      break;

    case 'set_session_name':
      emit({ id: command.id, type: 'response', command: 'set_session_name', success: true });
      break;

    case 'extension_ui_response':
      if (command.id === 'ui-1') {
        const answer = command.cancelled ? 'cancelled' : command.confirmed ? 'confirmed:true' : 'confirmed:false';
        emit({ type: 'message_update', assistantMessageEvent: { type: 'text_delta', delta: answer } });
        emit({ type: 'agent_end', messages: [], willRetry: false });
        emit({ type: 'agent_settled' });
      }
      break;

    case 'abort':
      if (holdRun) {
        emit({ id: command.id, type: 'response', command: 'abort', success: true });
        emit({ type: 'message_end', message: { role: 'assistant', stopReason: 'aborted' } });
        emit({ type: 'agent_end', messages: [], willRetry: false });
        emit({ type: 'agent_settled' });
        break;
      }
      emit({ id: command.id, type: 'response', command: 'abort', success: true });
      emit({ type: 'message_end', role: 'assistant', stopReason: 'aborted' });
      emit({ type: 'agent_settled' })
      break;

    default:
  }
});

if (surviveStdinClose) setInterval(() => {}, 60_000);
rl.on('close', () => {
  if (surviveStdinClose) return;
  process.exit(0);
});
