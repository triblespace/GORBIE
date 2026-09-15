//! Widgets for inspecting immutable TribleSpace data.

#[cfg(feature = "cluster-health")]
pub mod cluster_health;
pub mod entity_inspector;

#[cfg(feature = "cluster-health")]
pub use cluster_health::ClusterHealthWidget;
#[cfg(feature = "cluster-health")]
pub use cluster_health::cluster_health_notebook;
pub use entity_inspector::EntityInspectorResponse;
pub use entity_inspector::EntityInspectorStats;
pub use entity_inspector::EntityInspectorWidget;
pub use entity_inspector::EntityOrder;
pub use entity_inspector::id_full;
pub use entity_inspector::id_short;
