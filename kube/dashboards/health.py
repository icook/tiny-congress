"""Tiny Congress health dashboard — Grafana v1 schema via Foundation SDK."""

import grafana_foundation_sdk.builders.dashboard as db
import grafana_foundation_sdk.builders.timeseries as ts
import grafana_foundation_sdk.builders.stat as stat
import grafana_foundation_sdk.builders.gauge as gauge
import grafana_foundation_sdk.builders.logs as logs
import grafana_foundation_sdk.builders.prometheus as prom
import grafana_foundation_sdk.builders.loki as loki
from grafana_foundation_sdk.builders.common import VizLegendOptions as LegendBuilder
from grafana_foundation_sdk.models.common import DataSourceRef, LogsSortOrder
from grafana_foundation_sdk.models.dashboard import VariableOption

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

NS = "${namespace}"
PROM_DS = DataSourceRef(type_val="prometheus", uid="${prometheus_datasource}")
LOKI_DS = DataSourceRef(type_val="loki", uid="${loki_datasource}")

Pos = db.dashboard.GridPos
Threshold = db.dashboard.Threshold


def prom_target(expr: str, legend: str = "", ref: str = "A"):
    return prom.Dataquery().datasource(PROM_DS).expr(expr).legend_format(legend).ref_id(ref)


def loki_target(expr: str, ref: str = "A", max_lines: int = 200):
    return loki.Dataquery().datasource(LOKI_DS).expr(expr).max_lines(max_lines).ref_id(ref)


# ---------------------------------------------------------------------------
# Row 1 — Deployment Status (y=1, after row panel at y=0)
# ---------------------------------------------------------------------------

replica_count = (
    stat.Panel()
    .title("Replicas")
    .grid_pos(Pos(h=4, w=8, x=0, y=1))
    .with_target(prom_target(
        f'kube_deployment_status_replicas_available{{namespace="{NS}", deployment=~"tc-.*"}}',
        "{{deployment}}",
    ))
    .color_mode("background")
    .thresholds(db.ThresholdsConfig().steps([
        Threshold(color="red", value=None),
        Threshold(color="green", value=1.0),
    ]))
)

pod_restarts_24h = (
    stat.Panel()
    .title("Pod Restarts (24h)")
    .grid_pos(Pos(h=4, w=8, x=8, y=1))
    .with_target(prom_target(
        f'sum(increase(kube_pod_container_status_restarts_total{{namespace="{NS}"}}[24h]))',
    ))
    .color_mode("background")
    .thresholds(db.ThresholdsConfig().steps([
        Threshold(color="green", value=None),
        Threshold(color="orange", value=1.0),
        Threshold(color="red", value=5.0),
    ]))
)

uptime = (
    stat.Panel()
    .title("Uptime Since Deploy")
    .grid_pos(Pos(h=4, w=8, x=16, y=1))
    .with_target(prom_target(
        f'min(time() - kube_pod_start_time{{namespace="{NS}", pod=~"tc-.*"}})',
    ))
    .unit("s")
)

row1 = (
    db.Row("Deployment Status")
    .with_panel(replica_count)
    .with_panel(pod_restarts_24h)
    .with_panel(uptime)
)

# ---------------------------------------------------------------------------
# Row 2 — HTTP Health (y=6, after row1 header + 4h panels)
# ---------------------------------------------------------------------------

request_rate = (
    ts.Panel()
    .title("Request Rate by Status")
    .grid_pos(Pos(h=8, w=10, x=0, y=6))
    .with_target(prom_target(
        f'sum by (status_code) (rate(http_requests_total{{namespace="{NS}"}}[5m]))',
        "{{status_code}}",
    ))
    .line_width(2)
    .fill_opacity(10)
    .unit("reqps")
    .legend(LegendBuilder().display_mode("table").placement("right").show_legend(True))
)

latency = (
    ts.Panel()
    .title("Request Latency")
    .grid_pos(Pos(h=8, w=10, x=10, y=6))
    .with_target(prom_target(
        f'histogram_quantile(0.50, sum by (le) (rate(http_request_duration_seconds_bucket{{namespace="{NS}"}}[5m])))',
        "p50", "A",
    ))
    .with_target(prom_target(
        f'histogram_quantile(0.95, sum by (le) (rate(http_request_duration_seconds_bucket{{namespace="{NS}"}}[5m])))',
        "p95", "B",
    ))
    .with_target(prom_target(
        f'histogram_quantile(0.99, sum by (le) (rate(http_request_duration_seconds_bucket{{namespace="{NS}"}}[5m])))',
        "p99", "C",
    ))
    .line_width(2)
    .fill_opacity(5)
    .unit("s")
)

error_rate = (
    stat.Panel()
    .title("Error Rate %")
    .grid_pos(Pos(h=8, w=4, x=20, y=6))
    .with_target(prom_target(
        f'sum(rate(http_requests_total{{namespace="{NS}", status_code=~"5.."}}[5m])) / sum(rate(http_requests_total{{namespace="{NS}"}}[5m])) * 100',
    ))
    .unit("percent")
    .color_mode("background")
    .thresholds(db.ThresholdsConfig().steps([
        Threshold(color="green", value=None),
        Threshold(color="orange", value=1.0),
        Threshold(color="red", value=5.0),
    ]))
)

row2 = (
    db.Row("HTTP Health")
    .with_panel(request_rate)
    .with_panel(latency)
    .with_panel(error_rate)
)

# ---------------------------------------------------------------------------
# Row 3 — Resource Health (y=15, after row2 header + 8h panels)
# ---------------------------------------------------------------------------

cpu_usage = (
    ts.Panel()
    .title("CPU Usage vs Limit")
    .grid_pos(Pos(h=8, w=9, x=0, y=15))
    .with_target(prom_target(
        f'sum(rate(container_cpu_usage_seconds_total{{namespace="{NS}", container!=""}}[5m])) by (pod)',
        "{{pod}} usage", "A",
    ))
    .with_target(prom_target(
        f'sum(kube_pod_container_resource_limits{{namespace="{NS}", resource="cpu"}}) by (pod)',
        "{{pod}} limit", "B",
    ))
    .line_width(2)
    .fill_opacity(10)
    .unit("short")
)

memory_usage = (
    ts.Panel()
    .title("Memory Usage vs Limit")
    .grid_pos(Pos(h=8, w=9, x=9, y=15))
    .with_target(prom_target(
        f'sum(container_memory_working_set_bytes{{namespace="{NS}", container!=""}}) by (pod)',
        "{{pod}} usage", "A",
    ))
    .with_target(prom_target(
        f'sum(kube_pod_container_resource_limits{{namespace="{NS}", resource="memory"}}) by (pod)',
        "{{pod}} limit", "B",
    ))
    .line_width(2)
    .fill_opacity(10)
    .unit("bytes")
)

resource_pct = (
    gauge.Panel()
    .title("Resource % of Limit")
    .grid_pos(Pos(h=8, w=6, x=18, y=15))
    .with_target(prom_target(
        f'sum(rate(container_cpu_usage_seconds_total{{namespace="{NS}", container!=""}}[5m])) / sum(kube_pod_container_resource_limits{{namespace="{NS}", resource="cpu"}}) * 100',
        "CPU %", "A",
    ))
    .with_target(prom_target(
        f'sum(container_memory_working_set_bytes{{namespace="{NS}", container!=""}}) / sum(kube_pod_container_resource_limits{{namespace="{NS}", resource="memory"}}) * 100',
        "Memory %", "B",
    ))
    .unit("percent")
    .min(0)
    .max(100)
    .thresholds(db.ThresholdsConfig().steps([
        Threshold(color="green", value=None),
        Threshold(color="orange", value=70.0),
        Threshold(color="red", value=90.0),
    ]))
)

row3 = (
    db.Row("Resource Health")
    .with_panel(cpu_usage)
    .with_panel(memory_usage)
    .with_panel(resource_pct)
)

# ---------------------------------------------------------------------------
# Row 4 — Postgres Health (y=24, after row3 header + 8h panels)
# ---------------------------------------------------------------------------

pg_connections = (
    ts.Panel()
    .title("Active Connections")
    .grid_pos(Pos(h=8, w=9, x=0, y=24))
    .with_target(prom_target(
        f'pg_stat_activity_count{{namespace="{NS}", state="active"}}',
        "active",
    ))
    .line_width(2)
    .fill_opacity(10)
)

pg_pool_util = (
    stat.Panel()
    .title("Pool Utilization")
    .grid_pos(Pos(h=8, w=6, x=9, y=24))
    .with_target(prom_target(
        f'pg_stat_activity_count{{namespace="{NS}"}} / pg_settings_max_connections{{namespace="{NS}"}} * 100',
    ))
    .unit("percent")
    .color_mode("background")
    .thresholds(db.ThresholdsConfig().steps([
        Threshold(color="green", value=None),
        Threshold(color="orange", value=60.0),
        Threshold(color="red", value=85.0),
    ]))
)

pg_query_duration = (
    ts.Panel()
    .title("Query Duration")
    .grid_pos(Pos(h=8, w=9, x=15, y=24))
    .with_target(prom_target(
        f'rate(pg_stat_statements_mean_exec_time_ms{{namespace="{NS}"}}[5m])',
        "mean", "A",
    ))
    .line_width(2)
    .fill_opacity(5)
    .unit("ms")
)

row4 = (
    db.Row("Postgres Health")
    .with_panel(pg_connections)
    .with_panel(pg_pool_util)
    .with_panel(pg_query_duration)
)

# ---------------------------------------------------------------------------
# Row 5 — Logs (Loki) (y=33, after row4 header + 8h panels)
# ---------------------------------------------------------------------------

error_logs = (
    logs.Panel()
    .title("Recent Error Logs")
    .grid_pos(Pos(h=10, w=16, x=0, y=33))
    .with_target(loki_target(
        f'{{namespace="{NS}", container=~"tc-.*"}} |= "ERROR"',
        max_lines=200,
    ))
    .show_time(True)
    .sort_order(LogsSortOrder.DESCENDING)
    .wrap_log_message(True)
    .enable_log_details(True)
    .prettify_log_message(True)
)

error_rate_loki = (
    ts.Panel()
    .title("Error Rate (Loki)")
    .grid_pos(Pos(h=10, w=8, x=16, y=33))
    .with_target(loki_target(
        f'sum(count_over_time({{namespace="{NS}", container=~"tc-.*"}} |= "ERROR" [5m]))',
    ))
    .line_width(2)
    .fill_opacity(15)
    .color_scheme(db.FieldColor().mode("fixed").fixed_color("red"))
)

row5 = (
    db.Row("Logs (Loki)")
    .with_panel(error_logs)
    .with_panel(error_rate_loki)
)

# ---------------------------------------------------------------------------
# Assemble dashboard
# ---------------------------------------------------------------------------


def build() -> db.Dashboard:
    """Build the complete TC Health dashboard."""
    return (
        db.Dashboard("Tiny Congress Health")
        .tags(["tiny-congress", "auto-generated"])
        .time("now-3h", "now")
        .refresh("30s")
        .timepicker(
            db.TimePicker()
            .refresh_intervals(["10s", "30s", "1m", "5m", "15m"])
        )
        .with_variable(
            db.CustomVariable("namespace")
            .label("Namespace")
            .current(VariableOption(selected=True, text="tiny-congress-demo", value="tiny-congress-demo"))
        )
        .with_variable(
            db.DatasourceVariable("prometheus_datasource")
            .label("Prometheus")
            .type("prometheus")
        )
        .with_variable(
            db.DatasourceVariable("loki_datasource")
            .label("Loki")
            .type("loki")
        )
        .with_row(row1)
        .with_row(row2)
        .with_row(row3)
        .with_row(row4)
        .with_row(row5)
    )
