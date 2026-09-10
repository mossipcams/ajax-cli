// #1145: Codex exposes reasoning/Fast only after selecting a supporting model.
const fs = require('node:fs');
const readline = require('node:readline');

let model = 'gpt-6-astra';
let effort = 'medium';
let fast = true;
const choices = (values) => values.map((value) => ({ value, name: value }));
function configOptions() {
  const options = [{
    id: 'model', name: 'Model', category: 'model', type: 'select',
    currentValue: model, options: choices(['gpt-6-astra', 'gpt-5.6-sol']),
  }];
  if (model === 'gpt-5.6-sol') {
    options.push({
      id: 'reasoning_effort', name: 'Reasoning', category: 'thought_level',
      type: 'select', currentValue: effort, options: choices(['low', 'medium', 'high', 'max']),
    }, {
      id: 'fast-mode', name: 'Fast', category: 'model_config',
      type: 'boolean', currentValue: fast,
    });
  }
  return options;
}

readline.createInterface({ input: process.stdin }).on('line', (line) => {
  const { id, method, params } = JSON.parse(line);
  if (id === undefined) return;
  fs.appendFileSync('model-dependent-requests.jsonl', JSON.stringify({ method, params }) + '\n');
  let result = {};
  let error;
  if (method === 'initialize') {
    result = {
      protocolVersion: 1,
      agentCapabilities: { loadSession: true, sessionCapabilities: { resume: {} } },
      authMethods: [],
    };
  } else if (method === 'session/new') {
    result = { sessionId: 'model-dependent-session', configOptions: configOptions() };
  } else if (method === 'session/resume' || method === 'session/load') {
    if (method === 'session/resume' && params.sessionId === 'load-only') {
      error = { code: -32000, message: 'use session/load' };
    } else {
      result = { configOptions: configOptions() };
    }
  } else if (method === 'session/set_config_option') {
    const option = configOptions().find((option) => option.id === params.configId);
    const valid = option && (option.type === 'boolean'
      ? typeof params.value === 'boolean'
      : option.options.some((choice) => choice.value === params.value));
    if (!valid) {
      error = { code: -32602, message: 'option or value not advertised' };
    } else {
      if (params.configId === 'model') model = params.value;
      if (params.configId === 'reasoning_effort') effort = params.value;
      if (params.configId === 'fast-mode') fast = params.value;
      result = { configOptions: configOptions() };
    }
  } else {
    error = { code: -32601, message: 'unsupported method' };
  }
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, ...(error ? { error } : { result }) }) + '\n');
});
