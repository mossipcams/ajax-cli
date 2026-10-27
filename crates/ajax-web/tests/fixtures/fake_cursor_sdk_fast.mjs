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
