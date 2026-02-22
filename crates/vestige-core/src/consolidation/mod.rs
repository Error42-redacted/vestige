//! Memory Consolidation Module
//!
//! Implements sleep-inspired memory consolidation:
//! - Decay weak memories
//! - Promote emotional/important memories
//! - Generate embeddings
//! - Prune very weak memories (optional)
//! - 4-Phase biologically-accurate dream cycle (v2.0)

mod sleep;
pub mod phases;

pub use sleep::SleepConsolidation;
pub use phases::{
    DreamEngine, DreamPhase, FourPhaseDreamResult, PhaseResult,
    TriagedMemory, TriageCategory, CreativeConnection, CreativeConnectionType,
    DreamInsight,
};
