//! Executor dos scripts Luau das regras de reconhecimento (spec 13 §4, ADR-021).
//!
//! Costura (2026-11-17): implementa [`sbf_regras::script::ExecutorDeScript`] em dois
//! sabores — no próprio processo (só usado dentro do filho e nos testes) e num processo
//! filho do `sbf` (`sbf script --servir`), que é o que o app usa.
