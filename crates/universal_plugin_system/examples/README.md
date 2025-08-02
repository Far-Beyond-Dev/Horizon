# Universal Plugin System Examples

This directory contains examples demonstrating how to use the universal plugin system and how host applications should implement their own custom propagators.

## Files Overview

### Core Examples
- **`simple_test.rs`** - Basic event system usage with simple propagation
- **`working_plugin_test.rs`** - Plugin loading and lifecycle management
- **`host_app_example.rs`** - Complete host application integration
- **`horizon_integration_example.rs`** - Horizon-specific integration examples

### Event Classes and Custom Propagators
- **`event_classes_example.rs`** - Demonstrates different event classes with custom metadata
- **`common/application_propagators.rs`** - **Application-specific propagators implementation**

## Important: Application-Specific Propagators

The `common/application_propagators.rs` file contains examples of propagators that should **NOT** be part of the universal system itself, but should be implemented by host applications for their specific use cases:

- **GorcSpatialPropagator** - Spatial awareness for game engines
- **PriorityPropagator** - Priority-based event filtering  
- **RegionPropagator** - Region-based event filtering
- **SpatialPropagator** - Generic spatial event filtering
- **ChannelPropagator** - Channel-based network optimization
- **ClassAwarePropagator** - Routes events based on their class structure

## Universal System Propagators

The universal system only provides these basic, generic propagators:

- **AllEqPropagator** - Exact key matching (works with any key type)
- **DefaultPropagator** - Allows all events (debugging/broadcast)
- **DomainPropagator** - Filters by domain (first segment)
- **UniversalAllEqPropagator** - Universal exact matching for StructuredEventKey
- **CompositePropagator** - Combines multiple propagators with AND/OR logic

## Best Practices

1. **Keep the universal system generic** - Don't add application-specific logic
2. **Implement custom propagators in your application** - Use the traits provided
3. **Use event classes** - Structure your events with consistent metadata patterns
4. **Leverage composite propagators** - Combine multiple filters for complex logic
5. **Test propagation thoroughly** - Ensure events reach the right handlers

## Running Examples

```bash
# Basic usage
cargo run --example simple_test

# Event classes with custom propagators
cargo run --example event_classes_example

# Complete host application
cargo run --example host_app_example
```