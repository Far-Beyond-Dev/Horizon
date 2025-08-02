//! Metrics collection and reporting for production monitoring.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Metrics collector for server performance data
#[derive(Debug)]
pub struct MetricsCollector {
    /// Counter metrics (monotonically increasing values)
    counters: Arc<RwLock<HashMap<String, u64>>>,
    /// Gauge metrics (point-in-time values)
    gauges: Arc<RwLock<HashMap<String, f64>>>,
    /// Histogram metrics for latency/duration tracking
    histograms: Arc<RwLock<HashMap<String, Histogram>>>,
    /// Start time for uptime calculations
    start_time: Instant,
}

/// Histogram for tracking value distributions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Histogram {
    /// Number of samples
    pub count: u64,
    /// Sum of all values
    pub sum: f64,
    /// Minimum value seen
    pub min: f64,
    /// Maximum value seen
    pub max: f64,
    /// Buckets for percentile calculations
    pub buckets: Vec<HistogramBucket>,
}

/// Histogram bucket for percentile calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramBucket {
    /// Upper bound of this bucket
    pub upper_bound: f64,
    /// Count of values in this bucket
    pub count: u64,
}

impl MetricsCollector {
    /// Creates a new metrics collector
    pub fn new() -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
            gauges: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
        }
    }

    /// Increments a counter metric
    pub async fn increment_counter(&self, name: &str, value: u64) {
        let mut counters = self.counters.write().await;
        *counters.entry(name.to_string()).or_insert(0) += value;
    }

    /// Sets a gauge metric value
    pub async fn set_gauge(&self, name: &str, value: f64) {
        let mut gauges = self.gauges.write().await;
        gauges.insert(name.to_string(), value);
    }

    /// Records a histogram value
    pub async fn record_histogram(&self, name: &str, value: f64) {
        let mut histograms = self.histograms.write().await;
        let histogram = histograms.entry(name.to_string()).or_insert_with(|| Histogram::new());
        histogram.record(value);
    }

    /// Records the duration of an operation
    pub async fn record_duration<F, R>(&self, name: &str, operation: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        let start = Instant::now();
        let result = operation.await;
        let duration = start.elapsed().as_secs_f64();
        self.record_histogram(name, duration).await;
        result
    }

    /// Gets the current value of a counter
    pub async fn get_counter(&self, name: &str) -> u64 {
        self.counters.read().await.get(name).copied().unwrap_or(0)
    }

    /// Gets the current value of a gauge
    pub async fn get_gauge(&self, name: &str) -> Option<f64> {
        self.gauges.read().await.get(name).copied()
    }

    /// Gets a snapshot of all metrics
    pub async fn get_all_metrics(&self) -> MetricsSnapshot {
        let counters = self.counters.read().await.clone();
        let gauges = self.gauges.read().await.clone();
        let histograms = self.histograms.read().await.clone();

        MetricsSnapshot {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            counters,
            gauges,
            histograms,
        }
    }

    /// Exports metrics in Prometheus format
    pub async fn export_prometheus(&self) -> String {
        let counters = self.counters.read().await;
        let gauges = self.gauges.read().await;
        let histograms = self.histograms.read().await;

        let mut output = String::new();

        // Export counters
        for (name, value) in counters.iter() {
            output.push_str(&format!(
                "# TYPE {} counter\n{} {}\n",
                name, name, value
            ));
        }

        // Export gauges
        for (name, value) in gauges.iter() {
            output.push_str(&format!(
                "# TYPE {} gauge\n{} {}\n",
                name, name, value
            ));
        }

        // Export histograms
        for (name, histogram) in histograms.iter() {
            output.push_str(&format!(
                "# TYPE {} histogram\n",
                name
            ));
            output.push_str(&format!(
                "{}_count {}\n",
                name, histogram.count
            ));
            output.push_str(&format!(
                "{}_sum {}\n",
                name, histogram.sum
            ));

            for bucket in &histogram.buckets {
                output.push_str(&format!(
                    "{}_bucket{{le=\"{}\"}} {}\n",
                    name, bucket.upper_bound, bucket.count
                ));
            }
        }

        output
    }

    /// Records standard server metrics
    pub async fn record_server_metrics(&self, active_connections: usize, memory_mb: u64) {
        self.set_gauge("server_active_connections", active_connections as f64).await;
        self.set_gauge("server_memory_usage_mb", memory_mb as f64).await;
        self.set_gauge("server_uptime_seconds", self.start_time.elapsed().as_secs() as f64).await;
    }

    /// Records event system metrics
    pub async fn record_event_metrics(&self, handlers: usize, events_processed: u64) {
        self.set_gauge("event_handlers_registered", handlers as f64).await;
        self.increment_counter("events_processed_total", events_processed).await;
    }

    /// Records plugin metrics
    pub async fn record_plugin_metrics(&self, loaded_plugins: usize) {
        self.set_gauge("plugins_loaded", loaded_plugins as f64).await;
    }

    /// Records security metrics
    pub async fn record_security_metrics(&self, blocked_requests: u64, banned_ips: usize) {
        self.increment_counter("security_blocked_requests_total", blocked_requests).await;
        self.set_gauge("security_banned_ips", banned_ips as f64).await;
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of all metrics at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, f64>,
    pub histograms: HashMap<String, Histogram>,
}

impl Histogram {
    /// Creates a new histogram with default buckets
    pub fn new() -> Self {
        let buckets = vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]
        .into_iter()
        .map(|upper_bound| HistogramBucket {
            upper_bound,
            count: 0,
        })
        .collect();

        Self {
            count: 0,
            sum: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            buckets,
        }
    }

    /// Records a value in the histogram
    pub fn record(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        self.min = self.min.min(value);
        self.max = self.max.max(value);

        // Update buckets
        for bucket in &mut self.buckets {
            if value <= bucket.upper_bound {
                bucket.count += 1;
            }
        }
    }

    /// Calculates the average value
    pub fn average(&self) -> f64 {
        if self.count > 0 {
            self.sum / self.count as f64
        } else {
            0.0
        }
    }

    /// Estimates a percentile value
    pub fn percentile(&self, p: f64) -> f64 {
        if self.count == 0 {
            return 0.0;
        }

        let target_count = (self.count as f64 * p / 100.0) as u64;
        let mut cumulative_count = 0;

        for bucket in &self.buckets {
            cumulative_count += bucket.count;
            if cumulative_count >= target_count {
                return bucket.upper_bound;
            }
        }

        self.max
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro for timing operations and recording to histogram
#[macro_export]
macro_rules! time_operation {
    ($metrics:expr, $name:expr, $operation:expr) => {
        $metrics.record_duration($name, async { $operation }).await
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_counter_metrics() {
        let collector = MetricsCollector::new();
        
        collector.increment_counter("test_counter", 5).await;
        collector.increment_counter("test_counter", 3).await;
        
        assert_eq!(collector.get_counter("test_counter").await, 8);
        assert_eq!(collector.get_counter("nonexistent").await, 0);
    }

    #[tokio::test]
    async fn test_gauge_metrics() {
        let collector = MetricsCollector::new();
        
        collector.set_gauge("test_gauge", 42.5).await;
        assert_eq!(collector.get_gauge("test_gauge").await, Some(42.5));
        
        collector.set_gauge("test_gauge", 100.0).await;
        assert_eq!(collector.get_gauge("test_gauge").await, Some(100.0));
        
        assert_eq!(collector.get_gauge("nonexistent").await, None);
    }

    #[tokio::test]
    async fn test_histogram_metrics() {
        let collector = MetricsCollector::new();
        
        collector.record_histogram("test_histogram", 0.1).await;
        collector.record_histogram("test_histogram", 0.5).await;
        collector.record_histogram("test_histogram", 1.0).await;
        
        let snapshot = collector.get_all_metrics().await;
        let histogram = snapshot.histograms.get("test_histogram").unwrap();
        
        assert_eq!(histogram.count, 3);
        assert_eq!(histogram.sum, 1.6);
        assert_eq!(histogram.average(), 1.6 / 3.0);
    }

    #[tokio::test]
    async fn test_prometheus_export() {
        let collector = MetricsCollector::new();
        
        collector.increment_counter("requests_total", 100).await;
        collector.set_gauge("active_connections", 50.0).await;
        
        let prometheus_output = collector.export_prometheus().await;
        
        assert!(prometheus_output.contains("requests_total 100"));
        assert!(prometheus_output.contains("active_connections 50"));
        assert!(prometheus_output.contains("# TYPE"));
    }

    #[test]
    fn test_histogram_percentiles() {
        let mut histogram = Histogram::new();
        
        // Record values within the histogram's bucket range (0.001 to 10.0)
        for i in 1..=1000 {
            let value = (i as f64) / 1000.0; // Values from 0.001 to 1.0
            histogram.record(value);
        }
        
        // Test percentile calculations with meaningful assertions
        let p50 = histogram.percentile(50.0);
        let p95 = histogram.percentile(95.0);
        let p99 = histogram.percentile(99.0);
        
        assert!(p50 > 0.0);
        assert!(p95 >= p50); // Use >= to handle edge cases
        assert!(p99 >= p95); // Use >= to handle edge cases
    }
}