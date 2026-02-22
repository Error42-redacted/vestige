//! CognitiveEngine — Stateful neuroscience modules that persist across tool calls.
//!
//! v1.5.0: Wires ALL unused vestige-core features into the MCP server.
//! Each module is initialized once at startup and shared via Arc<Mutex<>>
//! across all tool invocations.

use vestige_core::{
    // Neuroscience modules
    ActivationNetwork, SynapticTaggingSystem, HippocampalIndex, ContextMatcher,
    AccessibilityCalculator, CompetitionManager, StateUpdateService,
    ImportanceSignals, NoveltySignal, ArousalSignal, RewardSignal, AttentionSignal,
    EmotionalMemory,
    // Advanced modules
    ImportanceTracker, ReconsolidationManager, IntentDetector, ActivityTracker,
    MemoryDreamer, MemoryChainBuilder, MemoryCompressor, CrossProjectLearner,
    AdaptiveEmbedder, SpeculativeRetriever, ConsolidationScheduler,
    // Search modules
    Reranker, RerankerConfig,
};
use vestige_core::search::TemporalSearcher;
use vestige_core::neuroscience::predictive_retrieval::PredictiveMemory;
use vestige_core::neuroscience::prospective_memory::{ProspectiveMemory, IntentionParser};

/// Stateful cognitive engine holding all neuroscience modules.
///
/// Lives on McpServer as `Arc<Mutex<CognitiveEngine>>` and is passed
/// to tools that need persistent cross-call state (search, ingest,
/// feedback, consolidation, new tools).
pub struct CognitiveEngine {
    // -- Neuroscience --
    pub activation_network: ActivationNetwork,
    pub synaptic_tagging: SynapticTaggingSystem,
    pub hippocampal_index: HippocampalIndex,
    pub context_matcher: ContextMatcher,
    pub accessibility_calc: AccessibilityCalculator,
    pub competition_mgr: CompetitionManager,
    pub state_service: StateUpdateService,
    pub importance_signals: ImportanceSignals,
    pub novelty_signal: NoveltySignal,
    pub arousal_signal: ArousalSignal,
    pub reward_signal: RewardSignal,
    pub attention_signal: AttentionSignal,
    pub emotional_memory: EmotionalMemory,
    pub predictive_memory: PredictiveMemory,
    pub prospective_memory: ProspectiveMemory,
    pub intention_parser: IntentionParser,

    // -- Advanced --
    pub importance_tracker: ImportanceTracker,
    pub reconsolidation: ReconsolidationManager,
    pub intent_detector: IntentDetector,
    pub activity_tracker: ActivityTracker,
    pub dreamer: MemoryDreamer,
    pub chain_builder: MemoryChainBuilder,
    pub compressor: MemoryCompressor,
    pub cross_project: CrossProjectLearner,
    pub adaptive_embedder: AdaptiveEmbedder,
    pub speculative_retriever: SpeculativeRetriever,
    pub consolidation_scheduler: ConsolidationScheduler,

    // -- Search --
    pub reranker: Reranker,
    pub temporal_searcher: TemporalSearcher,
}

impl Default for CognitiveEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitiveEngine {
    /// Initialize all cognitive modules with default configurations.
    pub fn new() -> Self {
        Self {
            // Neuroscience
            activation_network: ActivationNetwork::new(),
            synaptic_tagging: SynapticTaggingSystem::new(),
            hippocampal_index: HippocampalIndex::new(),
            context_matcher: ContextMatcher::new(),
            accessibility_calc: AccessibilityCalculator::default(),
            competition_mgr: CompetitionManager::new(),
            state_service: StateUpdateService::new(),
            importance_signals: ImportanceSignals::new(),
            novelty_signal: NoveltySignal::new(),
            arousal_signal: ArousalSignal::new(),
            reward_signal: RewardSignal::new(),
            attention_signal: AttentionSignal::new(),
            emotional_memory: EmotionalMemory::new(),
            predictive_memory: PredictiveMemory::new(),
            prospective_memory: ProspectiveMemory::new(),
            intention_parser: IntentionParser::new(),

            // Advanced
            importance_tracker: ImportanceTracker::new(),
            reconsolidation: ReconsolidationManager::new(),
            intent_detector: IntentDetector::new(),
            activity_tracker: ActivityTracker::new(),
            dreamer: MemoryDreamer::new(),
            chain_builder: MemoryChainBuilder::new(),
            compressor: MemoryCompressor::new(),
            cross_project: CrossProjectLearner::new(),
            adaptive_embedder: AdaptiveEmbedder::new(),
            speculative_retriever: SpeculativeRetriever::new(),
            consolidation_scheduler: ConsolidationScheduler::new(),

            // Search
            reranker: Reranker::new(RerankerConfig::default()),
            temporal_searcher: TemporalSearcher::new(),
        }
    }
}
