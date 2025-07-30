use super::*;

// Mock server context for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MockServerContext;

impl MockServerContext {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ServerContext for MockServerContext {
    fn events(&self) -> Arc<crate::system::EventSystem> {
        Arc::new(EventSystem::new())
    }

    fn region_id(&self) -> RegionId {
        RegionId::new()
    }

    fn log(&self, _level: LogLevel, _message: &str) {
        // Mock implementation
    }

    async fn send_to_player(&self, _player_id: PlayerId, _data: &[u8]) -> Result<(), ServerError> {
        Ok(())
    }

    async fn broadcast(&self, _data: &[u8]) -> Result<(), ServerError> {
        Ok(())
    }

    fn tokio_handle(&self) -> Option<tokio::runtime::Handle> {
        tokio::runtime::Handle::try_current().ok()
    }
}

#[tokio::test]
async fn test_complete_system_integration() {
    let server_context = Arc::new(MockServerContext::new());
    let (events, mut gorc_system) = create_complete_horizon_system(server_context).unwrap();

    // Test event registration
    events
        .on_core("test_event", |_: PlayerConnectedEvent| Ok(()))
        .await
        .unwrap();

    // Test GORC object registration
    let asteroid = ExampleAsteroid::new(Vec3::new(100.0, 0.0, 200.0), MineralType::Platinum);
    let _asteroid_id = gorc_system
        .register_object(asteroid, Vec3::new(100.0, 0.0, 200.0))
        .await;

    // Test player management
    let player_id = PlayerId::new();
    gorc_system
        .add_player(player_id, Vec3::new(50.0, 0.0, 180.0))
        .await;

    // Test system tick
    let result = gorc_system.tick().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_monitoring_system() {
    let events = create_simple_horizon_system();
    let mut monitor = HorizonMonitor::new(events.clone());

    // Generate initial report
    let report = monitor.generate_report().await;
    assert!(report.timestamp > 0);
    assert_eq!(report.uptime_seconds, 0); // Just started

    // Check alerts (should be none for new system)
    let alerts = monitor.should_alert().await;
    assert!(alerts.is_empty());
}
