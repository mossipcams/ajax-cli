#!/usr/bin/env node
// Minimal `pi --mode rpc` stand-in for jsonl_process tests. Reads LF-delimited
// JSONL commands from stdin, emits JSONL records on stdout in scripted order,
// and exits 0 when stdin closes (the documented orderly-shutdown path).
'use strict';

const readline = require('readline');

const sessionId = 'fake-pi-rpc-session-1';
// A `--session <id>` argv pair (as sent by resuming clients) overrides the
// fixed session id reported by get_state; without it behavior is unchanged.
// `--ignore-session` suppresses that override, so get_state keeps the fixed
// id even when --session is present (fail-closed restore fixture behavior).
const sessionArgIndex = process.argv.indexOf('--session');
const ignoreSession = process.argv.includes('--ignore-session');
const overriddenSessionId =
  !ignoreSession && sessionArgIndex >= 0 && typeof process.argv[sessionArgIndex + 1] === 'string'
    ? process.argv[sessionArgIndex + 1]
    : null;
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

function emit(record) {
  process.stdout.write(`${JSON.stringify(record)}\n`);
}

const flags = new Set(process.argv.slice(2));

// Handshake-mode switches. Each flag is independent; defaults keep the
// original fixture behavior for jsonl_process tests unchanged.
const stateFail = flags.has('--state-fail'); // get_state answers success:false
const noSessionId = flags.has('--no-session-id'); // get_state data lacks sessionId
const silentState = flags.has('--silent-state'); // get_state is never answered
const degradeOptionals = flags.has('--degrade-optionals'); // the three discovery commands fail
const interleaveEvent = flags.has('--interleave-event'); // one event mid-handshake
const rejectPrompt = flags.has('--reject-prompt'); // prompt answers success:false, no events
const holdRun = flags.has('--hold-run'); // prompt starts the run, which settles only on abort
const interleaveText = flags.has('--interleave-text'); // one text_delta mid-handshake
const silentStats = flags.has('--silent-stats'); // get_session_stats is never answered

// Scripted stdout output. Emitted before any command handling; libuv preserves
// write order on the pipe, so these lines precede every later record.
if (flags.has('--emit-u2028')) {
  // One JSON line containing U+2028 inside a string value. It must reach the
  // reader as exactly ONE record: JSONL framing splits only on LF, and the
  // parser must not treat U+2028/U+2029 as line separators.
  process.stdout.write('{"type":"note","text":"one\u2028two"}\n');
}
if (flags.has('--emit-garbage')) {
  // A stdout line that is not JSON at all; the reader must surface it as an
  // error record instead of dropping or panicking.
  process.stdout.write('this line is not json\n');
}
if (flags.has('--emit-stderr')) {
  // One short stderr write; it must land in the stderr tail and never in the
  // stdout record channel.
  process.stderr.write('fake pi rpc stderr noise\n');
}
if (flags.has('--emit-stderr-noise')) {
  // More than 4 KiB of stderr: the retained tail must keep only the last 4 KiB,
  // so the head marker below must be dropped.
  process.stderr.write('STDERR-HEAD-BEGIN ' + 'x'.repeat(8 * 1024) + ' STDERR-TAIL-END\n');
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });

rl.on('line', (raw) => {
  const text = raw.replace(/\r$/, '');
  let command;
  try {
    command = JSON.parse(text);
  } catch {
    return; // Malformed input framing is not exercised through this fixture.
  }
  if (!command || typeof command !== 'object') return;

  switch (command.type) {
    case 'get_state':
      if (silentState) {
        // Never answer: handshake tests expect the overall deadline to fire.
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
        model: fakeModel,
        thinkingLevel: 'medium',
        isStreaming: false,
        messageCount: 0,
      };
      // The sessionId is required by the handshake; --no-session-id omits it.
      // A --session <id> argv pair overrides the fixed id (resume path).
      if (!noSessionId) stateData.sessionId = overriddenSessionId ?? sessionId;
      emit({ id: command.id, type: 'response', command: 'get_state', success: true, data: stateData });
      if (interleaveEvent) {
        // A non-response record between the four handshake responses; the
        // handshake must collect it into pending, not drop it.
        emit({ type: 'extension_ui_request', action: 'menu', payload: { items: ['a'] } });
      }
      if (interleaveText) {
        // A mappable text record between the four handshake responses; the
        // session must surface it as its first step.
        emit({ type: 'message_update', assistantMessageEvent: { type: 'text_delta', delta: 'pending-text' } });
      }
      break;

    case 'get_available_models':
      if (degradeOptionals) {
        emit({ id: command.id, type: 'response', command: 'get_available_models', success: false, error: 'degraded for test' });
        break;
      }
      // Real Pi shape: data.models is an array of ModelInfo entries.
      emit({
        id: command.id,
        type: 'response',
        command: 'get_available_models',
        success: true,
        data: {
          models: [fakeModel, { id: 'fake-nonreasoning', name: 'Fake Non-reasoning', api: 'anthropic-messages', provider: 'fake-provider', reasoning: false, input: ['text', 'image'], contextWindow: 262144, maxTokens: 8192 }],
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
      // Real Pi shape: data.commands is an array of {name, description, source};
      // the last entry has no description to exercise the Option<String>.
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
        // Never answer: request tests expect the deadline to fire.
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
      if (holdRun) {
        // The run stops here (no agent_end, no agent_settled); it settles
        // only once an `abort` command arrives.
        break;
      }
      emit({ type: 'agent_end', messages: [], willRetry: false });
      emit({ type: 'agent_settled' })
      break;

    case 'abort':
      if (holdRun) {
        // Real Pi shape: stopReason is nested under `message`, and the run
        // settles with agent_end + agent_settled after the message ends.
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
      // Unknown commands are ignored on purpose so behavior stays deterministic.
  }
});

// Closing stdin is pi's documented orderly shutdown: exit cleanly with code 0.
rl.on('close', () => process.exit(0));
