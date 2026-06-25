import { describe, it, expect, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import App from "./App.svelte";

function setHash(hash: string) {
  window.location.hash = hash;
  window.dispatchEvent(new HashChangeEvent("hashchange"));
}

describe("App shell", () => {
  beforeEach(() => {
    window.location.hash = "";
  });

  it("renders the shared chrome", () => {
    const { getByRole, container } = render(App);
    expect(getByRole("heading", { name: "Ajax" })).toBeInTheDocument();
    expect(container.querySelector(".connection-status")).toBeInTheDocument();
    expect(container.querySelector(".update-banner")).toBeInTheDocument();
    expect(container.querySelector(".bottom-nav")).toBeInTheDocument();
    expect(container.querySelector("[data-bottom-action='new-task']")).toBeInTheDocument();
    expect(container.querySelector("main")).toBeInTheDocument();
  });

  it("shows the dashboard outlet by default", () => {
    const { container } = render(App);
    expect(container.querySelector("[data-outlet='dashboard']")).toBeInTheDocument();
    expect(container.querySelector("[data-outlet='settings']")).toBeNull();
  });

  it("shows the settings outlet on the settings route", async () => {
    const { container, findByTestId } = render(App);
    setHash("#/settings");
    expect(await findByTestId("outlet-settings")).toBeInTheDocument();
    expect(container.querySelector("[data-outlet='dashboard']")).toBeNull();
  });

  it("shows the task outlet on a task route", async () => {
    const { findByTestId } = render(App);
    setHash("#/t/web%2Ffix-login");
    expect(await findByTestId("outlet-task")).toBeInTheDocument();
  });
});
