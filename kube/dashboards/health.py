"""Tiny Congress health dashboard — Grafana v2 schema via Foundation SDK."""

import grafana_foundation_sdk.builders.dashboardv2beta1 as db
import grafana_foundation_sdk.builders.timeseries as ts_viz
import grafana_foundation_sdk.builders.stat as stat_viz
import grafana_foundation_sdk.builders.gauge as gauge_viz
import grafana_foundation_sdk.builders.logs as logs_viz
import grafana_foundation_sdk.builders.prometheus as prom
import grafana_foundation_sdk.builders.loki as loki
from grafana_foundation_sdk.models.common import DataSourceRef
from grafana_foundation_sdk.builders.common import VizLegendOptions as LegendBuilder
from grafana_foundation_sdk.models.dashboardv2beta1 import VariableOption

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

NS = "${namespace}"
PROM_DS = DataSourceRef(type_val="prometheus", uid="${prometheus_datasource}")
LOKI_DS = DataSourceRef(type_val="loki", uid="${loki_datasource}")


def prom_target(expr: str, legend: str = "", ref: str = "A") -> db.Target:
    q = prom.Dataquery().datasource(PROM_DS).expr(expr).legend_format(legend)
    return db.Target().query(q).ref_id(ref)


def loki_target(expr: str, ref: str = "A", max_lines: int = 200) -> db.Target:
    q = loki.Dataquery().datasource(LOKI_DS).expr(expr).max_lines(max_lines)
    return db.Target().query(q).ref_id(ref)


# ---------------------------------------------------------------------------
# Row 1 — Deployment Status
# ---------------------------------------------------------------------------

replica_count = (
    db.Panel()
    .title("Replicas")
    .data(db.QueryGroup().target(
        prom_target(
            f'kube_deployment_status_replicas_available{{namespace="{NS}", deployment=~"tc-.*"}}',
            "{{deployment}}",
        )
    ))
    .visualization(stat_viz.Visualization().color_mode("background").thresholds(
        db.ThresholdsConfig().steps([
            {"color": "red", "value": None},
            {"color": "green", "value": 1},
        ])
    ))
)

pod_restarts_24h = (
    db.Panel()
    .title("Pod Restarts (24h)")
    .data(db.QueryGroup().target(
        prom_target(
            f'sum(increase(kube_pod_container_status_restarts_total{{namespace="{NS}"}}[24h]))',
        )
    ))
    .visualization(stat_viz.Visualization().color_mode("background").thresholds(
        db.ThresholdsConfig().steps([
            {"color": "green", "value": None},
            {"color": "orange", "value": 1},
            {"color": "red", "value": 5},
        ])
    ))
)

uptime = (
    db.Panel()
    .title("Uptime Since Deploy")
    .data(db.QueryGroup().target(
        prom_target(
            f'min(time() - kube_pod_start_time{{namespace="{NS}", pod=~"tc-.*"}})',
        )
    ))
    .visualization(stat_viz.Visualization().unit("s"))
)

row1_elements = {
    "replicaCount": replica_count,
    "podRestarts24h": pod_restarts_24h,
    "uptime": uptime,
}

row1 = db.Row().title("Deployment Status").layout(
    db.Grid()
    .item(db.GridItem().name("replicaCount").width(8).height(4))
    .item(db.GridItem().name("podRestarts24h").width(8).height(4))
    .item(db.GridItem().name("uptime").width(8).height(4))
)

# ---------------------------------------------------------------------------
# Row 2 — HTTP Health
# ---------------------------------------------------------------------------

request_rate = (
    db.Panel()
    .title("Request Rate by Status")
    .data(db.QueryGroup().target(
        prom_target(
            f'sum by (status_code) (rate(http_requests_total{{namespace="{NS}"}}[5m]))',
            "{{status_code}}",
        )
    ))
    .visualization(
        ts_viz.Visualization()
        .line_width(2)
        .fill_opacity(10)
        .unit("reqps")
        .legend(LegendBuilder().display_mode("table").placement("right").show_legend(True))
    )
)

latency = (
    db.Panel()
    .title("Request Latency")
    .data(
        db.QueryGroup()
        .target(prom_target(
            f'histogram_quantile(0.50, sum by (le) (rate(http_request_duration_seconds_bucket{{namespace="{NS}"}}[5m])))',
            "p50", "A",
        ))
        .target(prom_target(
            f'histogram_quantile(0.95, sum by (le) (rate(http_request_duration_seconds_bucket{{namespace="{NS}"}}[5m])))',
            "p95", "B",
        ))
        .target(prom_target(
            f'histogram_quantile(0.99, sum by (le) (rate(http_request_duration_seconds_bucket{{namespace="{NS}"}}[5m])))',
            "p99", "C",
        ))
    )
    .visualization(
        ts_viz.Visualization()
        .line_width(2)
        .fill_opacity(5)
        .unit("s")
    )
)

error_rate = (
    db.Panel()
    .title("Error Rate %")
    .data(db.QueryGroup().target(
        prom_target(
            f'sum(rate(http_requests_total{{namespace="{NS}", status_code=~"5.."}}[5m])) / sum(rate(http_requests_total{{namespace="{NS}"}}[5m])) * 100',
        )
    ))
    .visualization(stat_viz.Visualization().unit("percent").color_mode("background").thresholds(
        db.ThresholdsConfig().steps([
            {"color": "green", "value": None},
            {"color": "orange", "value": 1},
            {"color": "red", "value": 5},
        ])
    ))
)

row2_elements = {
    "requestRate": request_rate,
    "latency": latency,
    "errorRate": error_rate,
}

row2 = db.Row().title("HTTP Health").layout(
    db.Grid()
    .item(db.GridItem().name("requestRate").width(10).height(8))
    .item(db.GridItem().name("latency").width(10).height(8))
    .item(db.GridItem().name("errorRate").width(4).height(8))
)

# ---------------------------------------------------------------------------
# Row 3 — Resource Health
# ---------------------------------------------------------------------------

cpu_usage = (
    db.Panel()
    .title("CPU Usage vs Limit")
    .data(
        db.QueryGroup()
        .target(prom_target(
            f'sum(rate(container_cpu_usage_seconds_total{{namespace="{NS}", container!=""}}[5m])) by (pod)',
            "{{pod}} usage", "A",
        ))
        .target(prom_target(
            f'sum(kube_pod_container_resource_limits{{namespace="{NS}", resource="cpu"}}) by (pod)',
            "{{pod}} limit", "B",
        ))
    )
    .visualization(ts_viz.Visualization().line_width(2).fill_opacity(10).unit("short"))
)

memory_usage = (
    db.Panel()
    .title("Memory Usage vs Limit")
    .data(
        db.QueryGroup()
        .target(prom_target(
            f'sum(container_memory_working_set_bytes{{namespace="{NS}", container!=""}}) by (pod)',
            "{{pod}} usage", "A",
        ))
        .target(prom_target(
            f'sum(kube_pod_container_resource_limits{{namespace="{NS}", resource="memory"}}) by (pod)',
            "{{pod}} limit", "B",
        ))
    )
    .visualization(ts_viz.Visualization().line_width(2).fill_opacity(10).unit("bytes"))
)

resource_pct = (
    db.Panel()
    .title("Resource % of Limit")
    .data(
        db.QueryGroup()
        .target(prom_target(
            f'sum(rate(container_cpu_usage_seconds_total{{namespace="{NS}", container!=""}}[5m])) / sum(kube_pod_container_resource_limits{{namespace="{NS}", resource="cpu"}}) * 100',
            "CPU %", "A",
        ))
        .target(prom_target(
            f'sum(container_memory_working_set_bytes{{namespace="{NS}", container!=""}}) / sum(kube_pod_container_resource_limits{{namespace="{NS}", resource="memory"}}) * 100',
            "Memory %", "B",
        ))
    )
    .visualization(gauge_viz.Visualization().unit("percent").min(0).max(100).thresholds(
        db.ThresholdsConfig().steps([
            {"color": "green", "value": None},
            {"color": "orange", "value": 70},
            {"color": "red", "value": 90},
        ])
    ))
)

row3_elements = {
    "cpuUsage": cpu_usage,
    "memoryUsage": memory_usage,
    "resourcePct": resource_pct,
}

row3 = db.Row().title("Resource Health").layout(
    db.Grid()
    .item(db.GridItem().name("cpuUsage").width(9).height(8))
    .item(db.GridItem().name("memoryUsage").width(9).height(8))
    .item(db.GridItem().name("resourcePct").width(6).height(8))
)

# ---------------------------------------------------------------------------
# Row 4 — Postgres Health
# ---------------------------------------------------------------------------

pg_connections = (
    db.Panel()
    .title("Active Connections")
    .data(db.QueryGroup().target(
        prom_target(
            f'pg_stat_activity_count{{namespace="{NS}", state="active"}}',
            "active",
        )
    ))
    .visualization(ts_viz.Visualization().line_width(2).fill_opacity(10))
)

pg_pool_util = (
    db.Panel()
    .title("Pool Utilization")
    .data(db.QueryGroup().target(
        prom_target(
            f'pg_stat_activity_count{{namespace="{NS}"}} / pg_settings_max_connections{{namespace="{NS}"}} * 100',
        )
    ))
    .visualization(stat_viz.Visualization().unit("percent").color_mode("background").thresholds(
        db.ThresholdsConfig().steps([
            {"color": "green", "value": None},
            {"color": "orange", "value": 60},
            {"color": "red", "value": 85},
        ])
    ))
)

pg_query_duration = (
    db.Panel()
    .title("Query Duration")
    .data(
        db.QueryGroup()
        .target(prom_target(
            f'rate(pg_stat_statements_mean_exec_time_ms{{namespace="{NS}"}}[5m])',
            "mean", "A",
        ))
    )
    .visualization(ts_viz.Visualization().line_width(2).fill_opacity(5).unit("ms"))
)

row4_elements = {
    "pgConnections": pg_connections,
    "pgPoolUtil": pg_pool_util,
    "pgQueryDuration": pg_query_duration,
}

row4 = db.Row().title("Postgres Health").layout(
    db.Grid()
    .item(db.GridItem().name("pgConnections").width(9).height(8))
    .item(db.GridItem().name("pgPoolUtil").width(6).height(8))
    .item(db.GridItem().name("pgQueryDuration").width(9).height(8))
)

# ---------------------------------------------------------------------------
# Row 5 — Logs (Loki)
# ---------------------------------------------------------------------------

error_logs = (
    db.Panel()
    .title("Recent Error Logs")
    .data(db.QueryGroup().target(
        loki_target(
            f'{{namespace="{NS}", container=~"tc-.*"}} |= "ERROR"',
            max_lines=200,
        )
    ))
    .visualization(
        logs_viz.Visualization()
        .show_time(True)
        .sort_order("Descending")
        .wrap_log_message(True)
        .enable_log_details(True)
        .prettify_log_message(True)
    )
)

error_rate_loki = (
    db.Panel()
    .title("Error Rate (Loki)")
    .data(db.QueryGroup().target(
        loki_target(
            f'sum(count_over_time({{namespace="{NS}", container=~"tc-.*"}} |= "ERROR" [5m]))',
        )
    ))
    .visualization(ts_viz.Visualization().line_width(2).fill_opacity(15).color_scheme(
        db.FieldColor().mode("fixed").fixed_color("red")
    ))
)

row5_elements = {
    "errorLogs": error_logs,
    "errorRateLoki": error_rate_loki,
}

row5 = db.Row().title("Logs (Loki)").layout(
    db.Grid()
    .item(db.GridItem().name("errorLogs").width(16).height(10))
    .item(db.GridItem().name("errorRateLoki").width(8).height(10))
)

# ---------------------------------------------------------------------------
# Assemble dashboard
# ---------------------------------------------------------------------------

all_elements = {}
for group in [row1_elements, row2_elements, row3_elements, row4_elements, row5_elements]:
    all_elements.update(group)


def build() -> db.Dashboard:
    """Build the complete TC Health dashboard."""
    d = (
        db.Dashboard("Tiny Congress Health")
        .tags(["tiny-congress", "auto-generated"])
        .time_settings(
            db.TimeSettings()
            .from_val("now-3h")
            .to("now")
            .auto_refresh("30s")
            .auto_refresh_intervals(["10s", "30s", "1m", "5m", "15m"])
        )
        .variable(
            db.CustomVariable("namespace")
            .label("Namespace")
            .current(VariableOption(selected=True, text="tiny-congress-demo", value="tiny-congress-demo"))
            .options([VariableOption(text="tiny-congress-demo", value="tiny-congress-demo")])
        )
        .variable(
            db.DatasourceVariable("prometheus_datasource")
            .label("Prometheus")
            .plugin_id("prometheus")
        )
        .variable(
            db.DatasourceVariable("loki_datasource")
            .label("Loki")
            .plugin_id("loki")
        )
        .layout(
            db.Rows()
            .row(row1)
            .row(row2)
            .row(row3)
            .row(row4)
            .row(row5)
        )
    )

    for name, panel in all_elements.items():
        d = d.element(name, panel)

    return d
