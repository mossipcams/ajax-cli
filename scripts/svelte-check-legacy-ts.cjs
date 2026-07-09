#!/usr/bin/env node
const Module = require("node:module");
const legacyTypeScript = require("typescript-5");
const load = Module._load;

// ponytail: svelte-check still needs the TS5 compiler API; drop when it supports TS7.
Module._load = function (request, parent, isMain) {
  if (request === "typescript") {
    return legacyTypeScript;
  }
  return load.call(this, request, parent, isMain);
};

require("svelte-check");
