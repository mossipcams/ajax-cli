// Fake @cursor/sdk variant for the cursor SDK fast-toggle persistence test:
// fake_cursor_sdk.mjs with a `fast` parameter added to grok-4.6. The shared
// fixture must stay untouched so the sidecar and effort tests see its original
// model list; this module only re-maps grok-4.6's parameters on top of it.

import { Agent, Cursor as BaseCursor } from "./fake_cursor_sdk.mjs";

export { Agent };

export const Cursor = {
  ...BaseCursor,
  models: {
    async list() {
      const models = await BaseCursor.models.list();
      return models.map((model) =>
        model.id === "grok-4.6"
          ? {
              ...model,
              parameters: [
                ...(model.parameters ?? []),
                { id: "fast", values: [{ value: "false" }, { value: "true" }] },
              ],
            }
          : model,
      );
    },
  },
};
