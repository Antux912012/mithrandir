pub mod safety;
pub mod local_llm;
pub mod client;
pub mod optimizer;

pub use safety::SafetyGuard;
pub use local_llm::{
    BuiltinLinuxAssistant, ChatMessage, GgufModelInfo, LlamaCppStatus, LocalLLMManager,
    get_recommended_nemotron_models,
};
pub use client::{AIClient, ChatResponse};
pub use optimizer::{AIOptimizerEngine, OptimizationRecommendation, OptimizationReport};
