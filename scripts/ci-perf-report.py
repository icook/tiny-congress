#!/usr/bin/env python3
"""CI performance report generator.

Usage:
    python3 scripts/ci-perf-report.py [RUN_ID]

If RUN_ID is omitted, uses the latest run on the current branch.

Environment variables:
    GRAFANA_URL     -- Grafana base URL (default: http://localhost:3000)
    GRAFANA_TOKEN   -- Grafana API token (falls back to gopass homelab/grafana-token)
    GH_REPO        -- GitHub repository (owner/repo) if not in a git repo

Prometheus queries are proxied through Grafana's datasource API — no separate
port-forward needed. Only requires:
    kubectl port-forward -n monitoring svc/monitoring-grafana 3000:80 &
"""

DEFAULT_GRAFANA_URL = "http://localhost:3000"

import argparse
import json
import os
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime
from typing import Optional


# ── GitHub helpers ─────────────────────────────────────────────────────────────

def gh_api(path: str) -> dict:
    """Call GitHub API via gh CLI subprocess."""
    result = subprocess.run(
        ["gh", "api", path],
        capture_output=True, text=True, check=True,
    )
    return json.loads(result.stdout)


def get_repo() -> str:
    """Get owner/repo from GH_REPO env or gh CLI."""
    if repo := os.environ.get("GH_REPO"):
        return repo
    result = subprocess.run(
        ["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"],
        capture_output=True, text=True, check=True,
    )
    return result.stdout.strip()


def get_latest_run_id(repo: str) -> str:
    """Get the latest ci.yml run on the current branch."""
    branch_result = subprocess.run(
        ["git", "rev-parse", "--abbrev-ref", "HEAD"],
        capture_output=True, text=True, check=True,
    )
    branch = branch_result.stdout.strip()
    data = gh_api(f"/repos/{repo}/actions/workflows/ci.yml/runs?branch={branch}&per_page=1")
    runs = data.get("workflow_runs", [])
    if not runs:
        raise SystemExit(f"No ci.yml runs found for branch '{branch}'")
    return str(runs[0]["id"])


def get_jobs(repo: str, run_id: str) -> list[dict]:
    """Fetch all jobs for a workflow run (handles pagination)."""
    jobs = []
    page = 1
    while True:
        data = gh_api(f"/repos/{repo}/actions/runs/{run_id}/jobs?per_page=100&page={page}")
        batch = data.get("jobs", [])
        jobs.extend(batch)
        if len(batch) < 100:
            break
        page += 1
    return jobs


# ── Time helpers ───────────────────────────────────────────────────────────────

def parse_time(s: Optional[str]) -> Optional[datetime]:
    if not s:
        return None
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def fmt_duration(seconds: float) -> str:
    if seconds < 0:
        return "N/A"
    m, s = divmod(int(seconds), 60)
    return f"{m}m{s:02d}s" if m else f"{s}s"


# ── Runner helpers ─────────────────────────────────────────────────────────────

def runner_tier(runner_name: Optional[str]) -> str:
    if not runner_name:
        return "unknown"
    name = runner_name.lower()
    if "large" in name:
        return "arc-large"
    if "small" in name:
        return "arc-small"
    if "arc" in name:
        return "self-hosted"
    return "gha-hosted"


def is_arc_runner(runner_name: Optional[str]) -> bool:
    return "arc" in (runner_name or "").lower()


def get_grafana_token() -> Optional[str]:
    """Get Grafana token from env or gopass."""
    token = os.environ.get("GRAFANA_TOKEN")
    if token:
        return token
    try:
        result = subprocess.run(
            ["gopass", "show", "homelab/grafana-token"],
            capture_output=True, text=True, check=True,
        )
        return result.stdout.strip() or None
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


# ── Grafana client ─────────────────────────────────────────────────────────────

class GrafanaClient:
    """Grafana API client — also proxies Prometheus queries via datasource API."""

    def __init__(self, base_url: str, token: str):
        self.base_url = base_url.rstrip("/")
        self.token = token
        self._prom_ds_uid: Optional[str] = None

    def _request(self, path: str, data: Optional[bytes] = None, method: str = "GET") -> dict:
        req = urllib.request.Request(
            f"{self.base_url}{path}",
            data=data,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.token}",
            },
            method=method,
        )
        with urllib.request.urlopen(req, timeout=10) as resp:
            return json.loads(resp.read())

    # ── Prometheus via datasource proxy ───────────────────────────────────────

    def _find_prometheus_uid(self) -> Optional[str]:
        """Find the UID of the first Prometheus datasource."""
        if self._prom_ds_uid is not None:
            return self._prom_ds_uid
        try:
            datasources = self._request("/api/datasources")
            for ds in datasources:
                if ds.get("type") == "prometheus":
                    self._prom_ds_uid = ds["uid"]
                    return self._prom_ds_uid
        except (urllib.error.URLError, KeyError, json.JSONDecodeError):
            pass
        return None

    def prom_query_range(
        self, query: str, start: datetime, end: datetime, step: str = "15"
    ) -> list:
        uid = self._find_prometheus_uid()
        if not uid:
            return []
        params = urllib.parse.urlencode({
            "query": query,
            "start": start.isoformat(),
            "end": end.isoformat(),
            "step": step,
        })
        try:
            data = self._request(f"/api/datasources/proxy/uid/{uid}/api/v1/query_range?{params}")
            if data.get("status") != "success":
                return []
            results = data.get("data", {}).get("result", [])
            if not results:
                return []
            return [(float(v[0]), float(v[1])) for v in results[0].get("values", [])]
        except (urllib.error.URLError, json.JSONDecodeError, KeyError, ValueError):
            return []

    def get_resource_stats(self, pod_name: str, start: datetime, end: datetime) -> dict:
        ns = "arc-runners"
        cpu_query = (
            f'sum by (pod) (rate(container_cpu_usage_seconds_total{{pod="{pod_name}",'
            f'container!="POD",namespace="{ns}"}}[5m]))'
        )
        mem_query = (
            f'sum by (pod) (container_memory_working_set_bytes{{pod="{pod_name}",'
            f'container!="POD",namespace="{ns}"}})'
        )
        cpu_values = self.prom_query_range(cpu_query, start, end)
        mem_values = self.prom_query_range(mem_query, start, end)

        def stats(values: list) -> tuple:
            if not values:
                return None, None
            vals = [v for _, v in values]
            return sum(vals) / len(vals), max(vals)

        cpu_avg, cpu_max = stats(cpu_values)
        mem_avg, mem_max = stats(mem_values)
        return {
            "cpu_avg": cpu_avg,
            "cpu_max": cpu_max,
            "mem_avg_mb": mem_avg / (1024 * 1024) if mem_avg is not None else None,
            "mem_max_mb": mem_max / (1024 * 1024) if mem_max is not None else None,
        }

    # ── Annotations ───────────────────────────────────────────────────────────

    def push_annotation(
        self, text: str, tags: list[str], start: datetime, end: datetime
    ) -> bool:
        payload = json.dumps({
            "text": text,
            "tags": tags,
            "time": int(start.timestamp() * 1000),
            "timeEnd": int(end.timestamp() * 1000),
        }).encode()
        try:
            self._request("/api/annotations", data=payload, method="POST")
            return True
        except urllib.error.URLError:
            return False


# ── Report sections ────────────────────────────────────────────────────────────

def compute_queue_delay(
    job: dict, run_started_at: datetime, jobs_by_name: dict
) -> float:
    """Estimate queue delay: started_at minus predecessor.completed_at.

    For most jobs, the predecessor is detect-changes. For jobs with other
    dependencies, this is still a rough estimate — the critical path section
    does proper dependency-aware calculation.
    """
    started = parse_time(job.get("started_at"))
    if not started:
        return -1.0

    # Use detect-changes completion as the baseline (every job depends on it).
    # Fall back to run_started_at only if detect-changes hasn't completed.
    dc = jobs_by_name.get("Detect changed paths") or jobs_by_name.get("detect-changes")
    if dc:
        dc_end = parse_time(dc.get("completed_at"))
        if dc_end:
            return max(0.0, (started - dc_end).total_seconds())

    return max(0.0, (started - run_started_at).total_seconds())


def fmt_columns(headers: list[str], rows: list[list[str]], sep: str = "  ") -> str:
    """Format rows as aligned columns for CLI output."""
    widths = [len(h) for h in headers]
    for row in rows:
        for i, cell in enumerate(row):
            widths[i] = max(widths[i], len(cell))
    parts = []
    header_line = sep.join(h.ljust(widths[i]) for i, h in enumerate(headers))
    parts.append(header_line)
    parts.append(sep.join("─" * widths[i] for i in range(len(headers))))
    for row in rows:
        parts.append(sep.join(cell.ljust(widths[i]) for i, cell in enumerate(row)))
    return "\n".join(parts)


def build_job_timing_table(jobs: list[dict], run_started_at: datetime) -> str:
    jobs_by_name = {j["name"]: j for j in jobs}
    rows = []
    for job in sorted(jobs, key=lambda j: j.get("started_at") or ""):
        started = parse_time(job.get("started_at"))
        completed = parse_time(job.get("completed_at"))
        runner = job.get("runner_name") or "N/A"
        tier = runner_tier(runner)
        conclusion = job.get("conclusion") or job.get("status", "pending")
        icon = {"success": "✓", "failure": "✗", "skipped": "–"}.get(conclusion, "?")
        exec_secs = (completed - started).total_seconds() if started and completed else -1.0
        queue_secs = compute_queue_delay(job, run_started_at, jobs_by_name)
        rows.append([job["name"], icon, fmt_duration(queue_secs), fmt_duration(exec_secs), runner, tier])

    return fmt_columns(["Job", "St", "Queue", "Exec", "Runner", "Tier"], rows)


def build_resource_table(
    jobs: list[dict], grafana_client: GrafanaClient
) -> str:
    arc_jobs = [
        j for j in jobs
        if is_arc_runner(j.get("runner_name")) and j.get("started_at") and j.get("completed_at")
    ]
    if not arc_jobs:
        return "_No ARC runner jobs with complete timing data._"

    rows = []
    for job in arc_jobs:
        pod = job.get("runner_name", "")
        s = parse_time(job["started_at"])
        c = parse_time(job["completed_at"])
        stats = grafana_client.get_resource_stats(pod, s, c)
        if stats["cpu_avg"] is None:
            print(
                f"  WARNING: No Prometheus data for pod {pod!r} — "
                "runner_name may not match pod name in arc-runners namespace",
                file=sys.stderr,
            )
        cpu_avg = f"{stats['cpu_avg']:.3f}" if stats["cpu_avg"] is not None else "N/A"
        cpu_max = f"{stats['cpu_max']:.3f}" if stats["cpu_max"] is not None else "N/A"
        mem_avg = f"{stats['mem_avg_mb']:.0f}" if stats["mem_avg_mb"] is not None else "N/A"
        mem_max = f"{stats['mem_max_mb']:.0f}" if stats["mem_max_mb"] is not None else "N/A"
        rows.append([job["name"], cpu_avg, cpu_max, mem_avg, mem_max])
    return fmt_columns(["Job", "CPU Avg", "CPU Max", "Mem Avg (MB)", "Mem Max (MB)"], rows)


def build_concurrency_section(jobs: list[dict]) -> str:
    events: list[tuple] = []
    for job in jobs:
        s = parse_time(job.get("started_at"))
        c = parse_time(job.get("completed_at"))
        if s:
            events.append((s, +1, job["name"]))
        if c:
            events.append((c, -1, job["name"]))
    events.sort(key=lambda e: e[0])

    count = 0
    max_count = 0
    max_time = None
    timeline = []
    for ts, delta, name in events:
        count += delta
        if count > max_count:
            max_count = count
            max_time = ts
        verb = "started" if delta > 0 else "ended"
        timeline.append(
            f"  {ts.strftime('%H:%M:%S')} {delta:+d} → {count} active  ({name} {verb})"
        )

    summary = f"Peak concurrency: {max_count} jobs"
    if max_time:
        summary += f" at {max_time.strftime('%H:%M:%S UTC')}"
    lines = [summary, "", "Timeline:", ""]
    lines.extend(timeline[:40])
    return "\n".join(lines)


def _parse_ci_yml(
    head_sha: str = "",
) -> Optional[dict[str, tuple[str, list[str]]]]:
    """Parse ci.yml to extract job definitions.

    When *head_sha* is provided, reads ci.yml from that commit via
    ``git show`` so the dep graph matches the workflow that actually ran.
    Falls back to the working-tree copy if the git command fails.

    Returns {job_id: (name_template, [needs_job_ids])} or None on failure.
    """
    try:
        import yaml
    except ImportError:
        return None

    raw: Optional[str] = None
    if head_sha:
        try:
            result = subprocess.run(
                ["git", "show", f"{head_sha}:.github/workflows/ci.yml"],
                capture_output=True, text=True, check=True,
            )
            raw = result.stdout
        except (subprocess.CalledProcessError, FileNotFoundError):
            pass  # fall through to working-tree read

    if raw is None:
        ci_path = os.path.join(
            os.path.dirname(__file__), "..", ".github", "workflows", "ci.yml"
        )
        try:
            with open(ci_path) as f:
                raw = f.read()
        except OSError:
            return None

    try:
        data = yaml.safe_load(raw)
    except yaml.YAMLError:
        return None

    jobs = data.get("jobs")
    if not isinstance(jobs, dict):
        return None

    result: dict[str, tuple[str, list[str]]] = {}
    for job_id, job_def in jobs.items():
        if not isinstance(job_def, dict):
            continue
        name = str(job_def.get("name", job_id))
        needs = job_def.get("needs", [])
        if isinstance(needs, str):
            needs = [needs]
        result[job_id] = (name, list(needs))
    return result


def _resolve_needs(
    needs_ids: list[str],
    ci_jobs: dict[str, tuple[str, list[str]]],
    jobs_by_name: dict[str, dict],
) -> list[str]:
    """Resolve a list of job IDs from ci.yml `needs:` to API display names.

    Matrix jobs (whose name templates contain `${{`) are expanded by matching
    API-returned job names against the template prefix.
    """
    resolved: list[str] = []
    for need_id in needs_ids:
        if need_id not in ci_jobs:
            continue
        need_template = ci_jobs[need_id][0]
        if "${{" in need_template:
            # Matrix job — match API names by prefix (e.g. "Build " matches
            # "Build tc-api-release", "Build postgres", etc.)
            prefix = need_template.split("${{")[0]
            resolved.extend(n for n in jobs_by_name if n.startswith(prefix))
        else:
            resolved.append(need_template)
    return resolved


def _build_dep_graph(
    jobs_by_name: dict[str, dict], head_sha: str = ""
) -> dict[str, list[str]]:
    """Build the CI job dependency graph by parsing ci.yml.

    Reads job definitions and `needs:` from the workflow file, resolving job IDs
    to the display names returned by the GitHub API. Matrix jobs are expanded by
    matching API names against the name template prefix.

    When *head_sha* is provided, reads ci.yml from that commit so the graph
    matches the workflow that actually ran (not the local working tree).
    """
    ci_jobs = _parse_ci_yml(head_sha)
    if ci_jobs is None:
        print(
            "WARNING: Could not parse ci.yml — critical path may be inaccurate",
            file=sys.stderr,
        )
        return {}

    deps: dict[str, list[str]] = {}
    for job_id, (name_template, needs_ids) in ci_jobs.items():
        resolved = _resolve_needs(needs_ids, ci_jobs, jobs_by_name)
        if "${{" in name_template:
            # Matrix job — find all matching API names and give each the
            # same resolved dependencies.
            prefix = name_template.split("${{")[0]
            for api_name in jobs_by_name:
                if api_name.startswith(prefix):
                    deps[api_name] = list(resolved)
        else:
            deps[name_template] = resolved

    return deps


def _compute_critical_path_jobs(
    jobs_by_name: dict[str, dict], head_sha: str = ""
) -> list[str]:
    """Return the critical path as a list of job names from first to CI Gate.

    Walks backwards from CI Gate, always following the predecessor that
    completed latest (the one that actually held up the next job).
    """
    deps = _build_dep_graph(jobs_by_name, head_sha)

    if "CI Gate" not in jobs_by_name:
        return []

    def completed_at_ts(name: str) -> float:
        j = jobs_by_name.get(name)
        if not j:
            return 0.0
        t = parse_time(j["completed_at"])
        return t.timestamp() if t else 0.0

    path: list[str] = []
    current = "CI Gate"
    while current:
        path.append(current)
        preds = deps.get(current, [])
        if not preds:
            break
        current = max(preds, key=completed_at_ts)
        if current not in jobs_by_name:
            break

    path.reverse()
    return path


def build_critical_path(
    jobs: list[dict], run_started_at: datetime, head_sha: str = ""
) -> str:
    """Format the critical path with per-job queue delay and execution time."""
    jobs_by_name: dict[str, dict] = {}
    for j in jobs:
        if j.get("started_at") and j.get("completed_at"):
            jobs_by_name[j["name"]] = j

    path = _compute_critical_path_jobs(jobs_by_name, head_sha)
    if not path:
        return "_CI Gate job not found — cannot compute critical path._"

    def completed_at_ts(name: str) -> float:
        j = jobs_by_name.get(name)
        if not j:
            return 0.0
        t = parse_time(j["completed_at"])
        return t.timestamp() if t else 0.0

    def job_duration(name: str) -> float:
        j = jobs_by_name.get(name)
        if not j:
            return 0.0
        s = parse_time(j["started_at"])
        c = parse_time(j["completed_at"])
        return (c - s).total_seconds() if s and c else 0.0

    rows = []
    for i, name in enumerate(path):
        dur = job_duration(name)
        j = jobs_by_name.get(name)
        if i == 0 or not j:
            q = 0.0
        else:
            pred_name = path[i - 1]
            pred_end = completed_at_ts(pred_name)
            job_start = parse_time(j["started_at"])
            if job_start and pred_end > 0:
                q = max(0.0, job_start.timestamp() - pred_end)
            else:
                q = 0.0
        rows.append([name, fmt_duration(q), fmt_duration(dur)])

    return fmt_columns(["Job", "Queue", "Exec"], rows)


def build_summary(jobs: list[dict], run_started_at: datetime, head_sha: str = "") -> str:
    completed = [j for j in jobs if j.get("completed_at")]
    if not completed:
        return "_Run still in progress._"

    run_end = max(parse_time(j["completed_at"]) for j in completed)
    started_times = [parse_time(j["started_at"]) for j in completed if j.get("started_at")]
    earliest_start = min(started_times) if started_times else run_started_at
    # Use the earlier of run_started_at and earliest job start — on re-runs,
    # run_started_at resets but job timestamps may precede it.
    effective_start = min(run_started_at, earliest_start)
    wall_time = (run_end - effective_start).total_seconds()

    compute_time = 0.0
    for job in completed:
        s = parse_time(job.get("started_at"))
        c = parse_time(job.get("completed_at"))
        if s and c:
            compute_time += (c - s).total_seconds()

    # Queue overhead = wall time minus total execution on critical path.
    # This is the time spent waiting for runners, not doing work.
    exec_on_crit_path = wall_time  # fallback
    jobs_by_name = {j["name"]: j for j in completed}
    crit_path = _compute_critical_path_jobs(jobs_by_name, head_sha)
    if crit_path:
        exec_on_crit_path = 0.0
        for name in crit_path:
            j = jobs_by_name.get(name)
            if j:
                s = parse_time(j.get("started_at"))
                c = parse_time(j.get("completed_at"))
                if s and c:
                    exec_on_crit_path += (c - s).total_seconds()
    queue_on_crit_path = max(0.0, wall_time - exec_on_crit_path)
    queue_pct = (queue_on_crit_path / wall_time * 100) if wall_time > 0 else 0

    rows = [
        ["Wall time", fmt_duration(wall_time)],
        ["Total compute time", fmt_duration(compute_time)],
        ["Queue overhead", f"{fmt_duration(queue_on_crit_path)} ({queue_pct:.0f}% of wall time)"],
    ]
    return fmt_columns(["Metric", "Value"], rows)


# ── Main ───────────────────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser(description="CI performance report")
    parser.add_argument(
        "run_id", nargs="?", default="",
        help="Workflow run ID (default: latest on current branch)",
    )
    args = parser.parse_args()

    try:
        repo = get_repo()
    except subprocess.CalledProcessError as exc:
        print(f"ERROR: Could not determine repo: {exc}", file=sys.stderr)
        sys.exit(1)

    run_id = args.run_id.strip()
    if not run_id:
        print("No run ID — fetching latest run on current branch...", file=sys.stderr)
        try:
            run_id = get_latest_run_id(repo)
        except subprocess.CalledProcessError as exc:
            print(f"ERROR: {exc}", file=sys.stderr)
            sys.exit(1)

    print(f"Analysing run {run_id} in {repo}", file=sys.stderr)

    try:
        run = gh_api(f"/repos/{repo}/actions/runs/{run_id}")
    except subprocess.CalledProcessError as exc:
        print(f"ERROR: Could not fetch run {run_id}: {exc}", file=sys.stderr)
        sys.exit(1)

    head_sha = run.get("head_sha", "")
    run_started_at = parse_time(run.get("run_started_at") or run.get("created_at"))
    if not run_started_at:
        print("ERROR: Could not parse run start time", file=sys.stderr)
        sys.exit(1)

    try:
        jobs = get_jobs(repo, run_id)
    except subprocess.CalledProcessError as exc:
        print(f"ERROR: Could not fetch jobs: {exc}", file=sys.stderr)
        sys.exit(1)

    # Optional Grafana integration — handles annotations + Prometheus proxy
    grafana_client: Optional[GrafanaClient] = None
    grafana_url = os.environ.get("GRAFANA_URL", DEFAULT_GRAFANA_URL)
    grafana_token = get_grafana_token()
    if grafana_token:
        try:
            # Probe connectivity
            req = urllib.request.Request(
                f"{grafana_url}/api/health",
                headers={"Authorization": f"Bearer {grafana_token}"},
            )
            urllib.request.urlopen(req, timeout=2)
            grafana_client = GrafanaClient(grafana_url, grafana_token)
            prom_uid = grafana_client._find_prometheus_uid()
            if prom_uid:
                print(f"Grafana: {grafana_url} (Prometheus datasource: {prom_uid})", file=sys.stderr)
            else:
                print(f"Grafana: {grafana_url} (no Prometheus datasource found)", file=sys.stderr)
        except (urllib.error.URLError, OSError):
            print(f"Grafana not reachable at {grafana_url} — skipping", file=sys.stderr)
    else:
        print("No Grafana token (set GRAFANA_TOKEN or install gopass) — skipping", file=sys.stderr)

    # ── Report output ──────────────────────────────────────────────────────────
    run_url = run.get("html_url", "")
    run_name = run.get("name", run_id)
    separator = "═" * 72

    print(f"\n{separator}")
    print(f"  CI Performance Report — Run {run_id}")
    print(separator)
    print(f"  Run:     {run_name}")
    print(f"  URL:     {run_url}")
    print(f"  Started: {run_started_at.strftime('%Y-%m-%d %H:%M:%S UTC')}")
    print(f"  Status:  {run.get('status', 'unknown')} / {run.get('conclusion', 'in-progress')}")
    print()

    print("── Job Timing ──────────────────────────────────────────────────────────")
    print()
    print(build_job_timing_table(jobs, run_started_at))
    print()

    arc_jobs = [j for j in jobs if is_arc_runner(j.get("runner_name")) and j.get("started_at") and j.get("completed_at")]
    if grafana_client and grafana_client._find_prometheus_uid() and arc_jobs:
        print("── Resource Usage ──────────────────────────────────────────────────────")
        print()
        print(build_resource_table(jobs, grafana_client))
        print()

    print("── Concurrency ─────────────────────────────────────────────────────────")
    print()
    print(build_concurrency_section(jobs))
    print()

    print("── Critical Path ───────────────────────────────────────────────────────")
    print()
    print(build_critical_path(jobs, run_started_at, head_sha))
    print()

    print("── Summary ─────────────────────────────────────────────────────────────")
    print()
    print(build_summary(jobs, run_started_at, head_sha))
    print()

    # Push Grafana annotations
    if grafana_client:
        print("Pushing Grafana annotations...", file=sys.stderr)
        timed_jobs = [j for j in jobs if j.get("started_at") and j.get("completed_at")]
        pushed = 0
        for job in timed_jobs:
            s = parse_time(job["started_at"])
            c = parse_time(job["completed_at"])
            tier = runner_tier(job.get("runner_name"))
            job_slug = job["name"].replace(" ", "-")
            tags = ["ci", f"run-{run_id}", job_slug, tier]
            text = f"CI job: {job['name']} ({tier})"
            if grafana_client.push_annotation(text, tags, s, c):
                pushed += 1
        print(f"Pushed {pushed}/{len(timed_jobs)} annotations to Grafana", file=sys.stderr)


if __name__ == "__main__":
    main()
