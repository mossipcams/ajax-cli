#!/usr/bin/env python3
"""Unit tests for gpu-ctl: promotion datasets, dataset-select, evals, status device/pipeline. Stdlib only."""
import contextlib, importlib.machinery, io, json, os, shutil, tempfile, unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
try:  # prefer /tmp; fall back to the repo's target/ if it is denied
    _T = tempfile.TemporaryDirectory()
except OSError:  # pragma: no cover - only hit when /tmp is unavailable
    tgt = HERE.parent.parent / "target"
    tgt.mkdir(parents=True, exist_ok=True)
    _T = tempfile.TemporaryDirectory(dir=str(tgt))

loader = importlib.machinery.SourceFileLoader("gpu_ctl", str(HERE / "gpu-ctl"))
gcu = loader.load_module()
_ORIG_JOB, _ORIG_BUSY = gcu.job, gcu.busy
_ORIG_PROCS = gcu.procs


def out(fn, *a):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        fn(*a)
    return json.loads(buf.getvalue())


RID_A = "20250701T000000Z-aaaaaaaa"  # newest
RID_B = "20250630T121111Z-bbbbbbbb"


class GpuCtlTest(unittest.TestCase):
    def setUp(self):
        # gcu is a dynamically loaded script module; every gcu.* assignment below is deliberate monkeypatching
        self.tmp = Path(_T.name)
        self.rundir = self.tmp / "run"
        self.state = self.tmp / "state"
        self.evals = self.tmp / "evals"
        for d in (self.state, self.evals):  # fresh per test: the shared temp root would leak fixtures
            shutil.rmtree(d, ignore_errors=True)
            d.mkdir(parents=True)
        self.rundir.mkdir(exist_ok=True)
        (self.rundir / "gpu.state").write_text("idle\n")
        setattr(gcu, "RUN", self.rundir)
        setattr(gcu, "JOB", self.rundir / "ajax-job.json")
        setattr(gcu, "STATE", self.state)
        setattr(gcu, "EVAL_RESULTS", self.evals)
        setattr(gcu, "HOST_ROOT", self.tmp)
        # keep docker/pgrep/ps out of the tests
        setattr(gcu, "runtime_up", lambda: False)
        setattr(gcu, "generating", lambda: False)
        setattr(gcu, "procs", lambda: [])
        setattr(gcu, "job", _ORIG_JOB)
        setattr(gcu, "busy", _ORIG_BUSY)  # per-test: earlier patches must not leak

    def _stage_dataset(self, rid, dataset=None, flags=()):
        rec = {"dataset_path": str(dataset), **{f: True for f in flags}}
        (self.state / f"{rid}.json").write_text(json.dumps(rec))
        return dataset

    def test_datasets_ordering_flags_current_promoted(self):
        da = self.tmp / "ds-a.jsonl"
        db = self.tmp / "ds-b.jsonl"
        da.write_text("x\n" * 10)
        self._stage_dataset(RID_A, da, flags=("validated",))
        self._stage_dataset(RID_B, db, flags=("validated", "canary_passed"))
        (self.state / "current.json").write_text(json.dumps({"run_id": RID_A}))
        (self.state / "promoted_dataset.json").write_text(json.dumps({"run_id": RID_B}))
        # noise that must be skipped: non-matching stems + unparsable record
        (self.state / "noise-selection.json").write_text("{}")
        (self.state / "20250702T000000Z-cccccccc.json").write_text("not json")
        r = out(gcu.datasets)
        self.assertEqual([e["run_id"] for e in r["datasets"]], [RID_A, RID_B])
        a, b = r["datasets"]
        self.assertTrue(a["current"])
        self.assertFalse(b["current"])
        self.assertFalse(a["promoted"])
        self.assertTrue(b["promoted"])
        self.assertEqual(a["size_bytes"], da.stat().st_size)
        self.assertIsNone(b["size_bytes"])  # dataset file missing
        self.assertTrue(a["validated"])
        self.assertFalse(a["canary_passed"])

    def test_dataset_select_rejects_bad_and_busy(self):
        for bad in ("../x", "nope"):
            with self.assertRaises(SystemExit) as cm:
                out(gcu.dataset_select, bad)
            self.assertEqual(cm.exception.code, 2)
        self._stage_dataset(RID_A)
        setattr(gcu, "busy", lambda: True)
        with self.assertRaises(SystemExit) as cm:
            out(gcu.dataset_select, RID_A)
        self.assertEqual(cm.exception.code, 3)

    def test_dataset_select_writes_current(self):
        self._stage_dataset(RID_B)
        r = out(gcu.dataset_select, RID_B)
        self.assertEqual(r, {"ok": True, "run_id": RID_B})
        self.assertEqual(json.loads((self.state / "current.json").read_text()), {"run_id": RID_B})

    def test_evals(self):
        # real host shapes: suite/gate_label live in metadata.json; total/passed/rates live under summary.summary
        runs = (
            ("checkpoint-600", "2026-10-22T13:43:57.169553+00:00", 24, 8, 0.3333,
             {"aliases": 0.0}, False, ["overall pass rate 0.3333 < 0.7"]),
            ("final", "2026-10-22T15:00:00.000000+00:00", 24, 18, 0.75,
             {"aliases": 0.5}, True, []),
        )
        for label, cr, total, passed, rate, cats, ok, failures in runs:
            d = self.evals / "group-a" / label
            d.mkdir(parents=True, exist_ok=True)
            (d / "metadata.json").write_text(json.dumps({
                "run_id": "20261022T134357Z-c72fa70f", "suite": "smoke",
                "checkpoint": f"/workspace/host/lfm/runs/sayso-promotion/X-canary/{label}",
                "created_at": cr, "gate_label": "smoke"}))
            (d / "summary.json").write_text(json.dumps({
                "summary": {"total": total, "passed": passed, "overall_pass_rate": rate, "category_pass_rate": cats},
                "promotion": {"passed": ok, "failures": failures}}))
        # skipped: summary without sibling metadata.json; unreadable summary
        orphan = self.evals / "group-b" / "orphan"
        orphan.mkdir(parents=True, exist_ok=True)
        (orphan / "summary.json").write_text("{}")
        broken = self.evals / "group-c" / "broken"
        broken.mkdir(parents=True, exist_ok=True)
        (broken / "metadata.json").write_text("{}")
        (broken / "summary.json").write_text("nope")
        r = out(gcu.evals)
        ids = [e["id"] for e in r["evals"]]
        self.assertEqual(ids, ["group-a/final", "group-a/checkpoint-600"])  # newest first, noise skipped
        fin, ck = r["evals"]
        self.assertEqual(fin["kind"], "final")
        self.assertEqual(ck["kind"], "checkpoint")
        self.assertEqual(ck["checkpoint_label"], "checkpoint-600")
        self.assertEqual(ck["suite"], "smoke")
        self.assertEqual(ck["gate_label"], "smoke")
        self.assertEqual(ck["total"], 24)
        self.assertEqual(ck["passed"], 8)
        self.assertEqual(ck["pass_rate"], 0.3333)
        self.assertIs(ck["promotion_passed"], False)
        self.assertTrue(ck["failures"])
        self.assertEqual(ck["category_pass_rate"], {"aliases": 0.0})
        self.assertTrue(fin["promotion_passed"])
        self.assertEqual(fin["failures"], [])
        self.assertEqual(fin["pass_rate"], 0.75)
        (self.state / "promoted_model.json").write_text(json.dumps(
            {"run_id": RID_A, "checkpoint": "final", "eval_metrics": {"overall_pass_rate": 0.75}}))
        self.assertEqual(r["champion"], None)  # champion written after this call
        r2 = out(gcu.evals)
        self.assertEqual(r2["champion"], {"run_id": RID_A, "checkpoint": "final", "pass_rates": {"promotion": 0.75}})

    def test_champion_formats(self):
        pm = self.state / "promoted_model.json"
        self.assertIsNone(gcu.champion())  # missing
        pm.write_text("not json")
        self.assertIsNone(gcu.champion())  # invalid
        pm.write_text(json.dumps({"run_id": RID_A, "checkpoint": "final", "eval_metrics": {
            "promotion": {"summary": {"overall_pass_rate": 0.6}, "promotion": {}},
            "context": {"summary": {"overall_pass_rate": 0.5}, "promotion": {}}}}))
        self.assertEqual(gcu.champion()["pass_rates"], {"promotion": 0.6, "context": 0.5})
        pm.write_text(json.dumps({"run_id": RID_A, "checkpoint": "final",
                                  "eval_metrics": {"overall_pass_rate": 0.6083, "total": 120}}))
        self.assertEqual(gcu.champion()["pass_rates"], {"promotion": 0.6083})
        pm.write_text(json.dumps({"run_id": RID_A, "checkpoint": "final", "eval_metrics": None}))
        self.assertEqual(gcu.champion()["pass_rates"], {})

    def test_status_pipeline_device(self):
        s = out(gcu.status)
        self.assertIsNone(s["pipeline"])
        self.assertIsNone(s["device"])
        # current dataset with a missing record: flags all False
        (self.state / "current.json").write_text(json.dumps({"run_id": RID_B}))
        s = out(gcu.status)
        self.assertEqual(s["pipeline"]["run_id"], RID_B)
        self.assertFalse(any(s["pipeline"][f] for f in gcu.STAGES))
        # training state -> gpu + flags from the record
        (self.state / f"{RID_B}.json").write_text(json.dumps({"validated": True, "trained": True}))
        (self.rundir / "gpu.state").write_text("train:ajax 1 2 1700000000\n")
        s = out(gcu.status)
        self.assertEqual(s["device"], "gpu")
        self.assertTrue(s["pipeline"]["validated"])
        self.assertFalse(s["pipeline"]["canary_passed"])

    def test_status_device_cpu_when_job_running(self):
        setattr(gcu, "job", lambda: {"kind": "lfm-eval", "started": "", "log": str(self.rundir / "nope.log"),
                                     "running": True})
        s = out(gcu.status)
        self.assertEqual(s["device"], "cpu")

    def test_procs_parses_one_ps_call_and_returns_empty_on_error(self):
        with patch.object(gcu.subprocess, "run") as ps:
            ps.return_value.stdout = " 123 42 python3 ./sayso eval\n456 7 python worker.py\n"
            self.assertEqual(_ORIG_PROCS(), [(123, 42, ["python3", "./sayso", "eval"]),
                                            (456, 7, ["python", "worker.py"])])
            ps.assert_called_once_with(["ps", "-eo", "pid=,etimes=,args="], capture_output=True, text=True, check=True)
            ps.return_value.stdout = "bad ps output"
            self.assertEqual(_ORIG_PROCS(), [])
            for error in (OSError(), gcu.subprocess.CalledProcessError(1, "ps")):
                ps.side_effect = error
                self.assertEqual(_ORIG_PROCS(), [])

    def test_external_sayso_status_and_busy(self):
        for cmd in ("generate", "validate", "canary", "promote-dataset", "train", "eval",
                    "promote-model", "select-checkpoint"):
            with self.subTest(cmd=cmd):
                setattr(gcu, "procs", lambda: [(123, 42, ["python3", "./sayso", cmd])])
                self.assertEqual(gcu.external_sayso(), {"kind": cmd, "elapsed_s": 42})
                s = out(gcu.status)
                self.assertEqual(s["run"], {"kind": cmd, "started": "", "running": True,
                                            "progress": None, "log_tail": [], "external": True})
                self.assertEqual(s["device"], "cpu")
                self.assertTrue(gcu.busy())
        setattr(gcu, "procs", lambda: [(1, 0, ["python3", "./sayso", "unknown"]),
                                       (2, 0, ["python3", "./sayso"])])
        self.assertIsNone(gcu.external_sayso())

    def test_procs_skips_only_malformed_lines(self):
        with patch.object(gcu.subprocess, "run") as ps:
            ps.return_value.stdout = ("123 42 python3 ./sayso eval\n\n456 7\n"
                                      "bad 7 python worker.py\n789 bad python worker.py\n"
                                      "987 8 python worker.py\n")
            self.assertEqual(_ORIG_PROCS(), [(123, 42, ["python3", "./sayso", "eval"]),
                                            (987, 8, ["python", "worker.py"])])
            ps.assert_called_once_with(["ps", "-eo", "pid=,etimes=,args="], capture_output=True, text=True, check=True)

    def test_external_work_refuses_mutations(self):
        setattr(gcu, "procs", lambda: [(123, 42, ["python3", "./sayso", "eval"])])
        self._stage_dataset(RID_A)
        with patch.object(gcu, "profiles", return_value=["llama"]):
            for fn, arg in ((gcu.start, "generate"), (gcu.profile_set, "llama"), (gcu.dataset_select, RID_A)):
                with self.subTest(verb=fn.__name__), self.assertRaises(SystemExit) as cm:
                    out(fn, arg)
                self.assertEqual(cm.exception.code, 3)

    def test_evals_in_flight_labels_and_device(self):
        argv = ["python", "/workspace/host/training/scripts/eval_checkpoint_cpu.py",
                "/workspace/host/runs/checkpoint-600", "--suite", "promotion", "--tag", "gold",
                "--tag", "shadow", "--profile", "llama", "--out", "/workspace/host/evals/group-a"]
        setattr(gcu, "procs", lambda: [(123, 42, argv)])
        group = self.evals / "group-a"
        finished = group / "timestamp-promotion"
        finished.mkdir(parents=True)
        (finished / "summary.json").write_text("{}")
        (finished / "metadata.json").write_text(json.dumps({"gate_label": "promotion"}))
        pending = group / "gold"
        pending.mkdir()
        (pending / "metadata.json").write_text(json.dumps({"gate_label": "gold"}))
        orphan = group / "shadow"
        orphan.mkdir()
        (orphan / "summary.json").write_text("{}")
        expected = [{"checkpoint_label": "checkpoint-600", "group": "group-a",
                     "labels": ["promotion", "gold", "shadow"], "done": ["promotion"],
                     "current": "gold", "elapsed_s": 42}]
        self.assertEqual(gcu.evals_in_flight(), expected)
        self.assertTrue(gcu.busy())
        setattr(gcu, "generating", lambda: True)
        setattr(gcu, "runtime_up", lambda: True)
        s = out(gcu.status)
        self.assertEqual(s["eval_progress"], expected)
        self.assertEqual(s["device"], "cpu")
        (self.rundir / "gpu.state").write_text("train:lfm\n")
        self.assertEqual(out(gcu.status)["device"], "gpu")
        (pending / "summary.json").write_text("{}")
        (orphan / "metadata.json").write_text(json.dumps({"gate_label": "shadow"}))
        self.assertEqual(gcu.evals_in_flight()[0]["done"], ["promotion", "gold", "shadow"])
        self.assertIsNone(gcu.evals_in_flight()[0]["current"])
        argv[-1] = str(group)  # host-native paths work too
        self.assertIsNone(gcu.evals_in_flight()[0]["current"])
        for invalid in (argv[:2], argv[:4], argv[:-1], [*argv[:4], "invalid", *argv[5:]],
                        ["python", "unrelated.py", *argv[2:]]):
            setattr(gcu, "procs", lambda: [(123, 42, invalid)])
            self.assertEqual(gcu.evals_in_flight(), [])

    def test_evals_in_flight_skips_docker_and_deduplicates(self):
        argv = ["python", "/workspace/host/training/scripts/eval_checkpoint_cpu.py",
                "/workspace/host/runs/checkpoint-600", "--suite", "promotion",
                "--out", "/workspace/host/evals/group-a"]
        expected = [{"checkpoint_label": "checkpoint-600", "group": "group-a",
                     "labels": ["promotion"], "done": [], "current": "promotion", "elapsed_s": 42}]
        for docker in ("docker", "/usr/bin/docker"):
            with self.subTest(docker=docker):
                setattr(gcu, "procs", lambda: [(1, 43, [docker, "exec", "unsloth", *argv]), (2, 42, argv)])
                self.assertEqual(gcu.evals_in_flight(), expected)
                setattr(gcu, "procs", lambda: [(1, 43, [docker, "exec", "unsloth", *argv])])
                self.assertEqual(gcu.evals_in_flight(), [])
        setattr(gcu, "procs", lambda: [(2, 42, argv), (3, 41, argv)])
        self.assertEqual(gcu.evals_in_flight(), expected)
        other_group = [*argv[:-1], "/workspace/host/evals/group-b"]
        other_checkpoint = [*argv[:2], "/workspace/host/runs/checkpoint-900", *argv[3:]]
        setattr(gcu, "procs", lambda: [(2, 42, argv), (3, 41, other_group), (4, 40, other_checkpoint)])
        self.assertEqual([(e["group"], e["checkpoint_label"]) for e in gcu.evals_in_flight()],
                         [("group-a", "checkpoint-600"), ("group-b", "checkpoint-600"), ("group-a", "checkpoint-900")])

    def test_device_generation_and_idle(self):
        s = out(gcu.status)
        self.assertIsNone(s["device"])
        self.assertEqual(s["eval_progress"], [])
        self.assertFalse(gcu.busy())
        setattr(gcu, "generating", lambda: True)
        for up, device in ((True, "gpu"), (False, "cpu")):
            setattr(gcu, "runtime_up", lambda: up)
            self.assertEqual(out(gcu.status)["device"], device)

    def test_page_and_gpu_lock_runs_take_precedence(self):
        setattr(gcu, "procs", lambda: [(123, 42, ["python3", "./sayso", "eval"])])
        setattr(gcu, "job", lambda: {"kind": "validate", "started": "", "log": str(self.rundir / "nope.log"),
                                     "running": True})
        s = out(gcu.status)
        self.assertEqual(s["run"]["kind"], "validate")
        self.assertFalse(s["run"]["external"])
        setattr(gcu, "job", lambda: {"kind": "validate", "started": "", "log": str(self.rundir / "nope.log"),
                                     "running": False})
        self.assertTrue(out(gcu.status)["run"]["external"])
        (self.rundir / "gpu.state").write_text("train:lfm\n")
        s = out(gcu.status)
        self.assertEqual(s["run"]["kind"], "lfm-train")
        self.assertFalse(s["run"]["external"])

    def test_status_succeeds_when_current_json_null(self):
        (self.state / "current.json").write_text("null")
        s = out(gcu.status)
        self.assertIsNone(s["pipeline"])

    def test_status_succeeds_when_run_record_not_object(self):
        (self.state / "current.json").write_text(json.dumps({"run_id": RID_B}))
        (self.state / f"{RID_B}.json").write_text("[]")
        s = out(gcu.status)
        self.assertEqual(s["pipeline"]["run_id"], RID_B)
        self.assertFalse(any(s["pipeline"][f] for f in gcu.STAGES))

    def test_datasets_skips_non_object_run_record(self):
        self._stage_dataset(RID_A)
        (self.state / f"{RID_B}.json").write_text('"x"')
        r = out(gcu.datasets)
        self.assertEqual([e["run_id"] for e in r["datasets"]], [RID_A])

    def test_evals_skips_eval_when_metadata_null(self):
        d = self.evals / "group-a" / "checkpoint-1"
        d.mkdir(parents=True, exist_ok=True)
        (d / "metadata.json").write_text("null")
        (d / "summary.json").write_text(json.dumps({"summary": {}, "promotion": {}}))
        r = out(gcu.evals)
        self.assertEqual(r["evals"], [])

    def test_champion_none_when_promoted_model_not_object(self):
        (self.state / "promoted_model.json").write_text("[]")
        self.assertIsNone(gcu.champion())
        r = out(gcu.evals)
        self.assertIsNone(r["champion"])

    def test_promoted_run_id_in_status_pipeline_present(self):
        self._stage_dataset(RID_B)
        (self.state / "current.json").write_text(json.dumps({"run_id": RID_A}))
        (self.state / "promoted_dataset.json").write_text(json.dumps({"run_id": RID_B}))
        s = out(gcu.status)
        self.assertEqual(s["pipeline"]["promoted_run_id"], RID_B)
        self.assertTrue(s["pipeline"]["promoted_record_present"])

    def test_promoted_run_id_in_status_pipeline_missing_record(self):
        (self.state / "current.json").write_text(json.dumps({"run_id": RID_A}))
        (self.state / "promoted_dataset.json").write_text(json.dumps({"run_id": RID_B}))
        s = out(gcu.status)
        self.assertEqual(s["pipeline"]["promoted_run_id"], RID_B)
        self.assertFalse(s["pipeline"]["promoted_record_present"])

    def test_promoted_run_id_in_datasets_output(self):
        self._stage_dataset(RID_B)
        (self.state / "promoted_dataset.json").write_text(json.dumps({"run_id": RID_B}))
        r = out(gcu.datasets)
        self.assertEqual(r["promoted_run_id"], RID_B)
        self.assertTrue(r["promoted_record_present"])
        (self.state / "promoted_dataset.json").write_text("null")
        r2 = out(gcu.datasets)
        self.assertIsNone(r2["promoted_run_id"])
        self.assertFalse(r2["promoted_record_present"])

    def test_jobs_includes_select_checkpoint(self):
        self.assertEqual(gcu.JOBS["select-checkpoint"], ["./sayso", "select-checkpoint"])

    def test_checkpoint_eval_supported_from_sayso(self):
        training = self.tmp / "training"
        training.mkdir(parents=True, exist_ok=True)
        setattr(gcu, "TRAINING", training)
        (training / "sayso").write_text("# no checkpoint command\n")
        self.assertFalse(gcu.checkpoint_eval_supported())
        (training / "sayso").write_text("select-checkpoint\n")
        self.assertTrue(gcu.checkpoint_eval_supported())

    def test_status_checkpoint_eval(self):
        training = self.tmp / "training"
        training.mkdir(parents=True, exist_ok=True)
        setattr(gcu, "TRAINING", training)
        (training / "sayso").write_text("select-checkpoint\n")
        s = out(gcu.status)
        self.assertTrue(s["checkpoint_eval"])
        (training / "sayso").write_text("train only\n")
        s = out(gcu.status)
        self.assertFalse(s["checkpoint_eval"])

    def test_start_select_checkpoint_unsupported(self):
        training = self.tmp / "training"
        training.mkdir(parents=True, exist_ok=True)
        setattr(gcu, "TRAINING", training)
        (training / "sayso").write_text("train only\n")
        with self.assertRaises(SystemExit) as cm:
            gcu.start("select-checkpoint")
        self.assertEqual(cm.exception.code, 2)


if __name__ == "__main__":
    unittest.main(verbosity=2)
