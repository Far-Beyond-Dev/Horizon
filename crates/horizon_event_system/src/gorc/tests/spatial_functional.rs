use std::collections::HashSet;

use crate::gorc::spatial::{QueryFilters, SpatialPartition, SpatialQuery};
use crate::types::{PlayerId, Position};

#[tokio::test]
async fn spatial_partition_radius_query_returns_expected_players() {
    let partition = SpatialPartition::new();

    let near_player = PlayerId::new();
    let border_player = PlayerId::new();
    let far_player = PlayerId::new();

    partition
        .update_player_position(near_player, Position::new(0.0, 0.0, 0.0))
        .await;
    partition
        .update_player_position(border_player, Position::new(9.5, 0.0, 0.0))
        .await;
    partition
        .update_player_position(far_player, Position::new(25.0, 0.0, 0.0))
        .await;

    let results = partition
        .query_radius(Position::new(0.0, 0.0, 0.0), 10.0)
        .await;
    let ids: HashSet<PlayerId> = results.into_iter().map(|r| r.player_id).collect();

    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&near_player));
    assert!(ids.contains(&border_player));
    assert!(!ids.contains(&far_player));
}

#[tokio::test]
async fn spatial_partition_removes_players_from_index() {
    let partition = SpatialPartition::new();
    let player = PlayerId::new();

    partition
        .update_player_position(player, Position::new(50.0, 50.0, 0.0))
        .await;
    assert_eq!(partition.player_count().await, 1);

    partition.remove_player(player).await;

    assert_eq!(partition.player_count().await, 0);
    let results = partition
        .query_radius(Position::new(50.0, 50.0, 0.0), 5.0)
        .await;
    assert!(results.is_empty());
}

#[tokio::test]
async fn spatial_partition_respects_query_filters() {
    let partition = SpatialPartition::new();
    let include_player = PlayerId::new();
    let excluded_player = PlayerId::new();
    let filtered_out_player = PlayerId::new();

    partition
        .update_player_position(include_player, Position::new(0.0, 0.0, 0.0))
        .await;
    partition
        .update_player_position(excluded_player, Position::new(1.0, 0.0, 0.0))
        .await;
    partition
        .update_player_position(filtered_out_player, Position::new(2.0, 0.0, 0.0))
        .await;

    let mut filters = QueryFilters::default();
    filters.include_players = Some(HashSet::from([include_player, excluded_player]));
    filters.exclude_players = Some(HashSet::from([excluded_player]));
    filters.max_results = Some(1);

    let query = SpatialQuery {
        center: Position::new(0.0, 0.0, 0.0),
        radius: 10.0,
        filters,
    };

    let results = partition.query(query).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].player_id, include_player);
}
