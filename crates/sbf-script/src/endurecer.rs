//! Endurecimento do processo filho (Linux, melhor esforço).
//!
//! O que faz, nesta ordem, e cada passo independe do outro:
//!
//! 1. **Landlock**: o processo abre mão de todo acesso a arquivos (ruleset sem nenhuma
//!    regra de caminho = nega tudo) e, se o kernel tiver a ABI 4 ou mais, de conectar e
//!    escutar em portas TCP. Implica `no_new_privs`. Os descritores já abertos (stdin e
//!    stdout do protocolo) continuam valendo.
//! 2. **rlimits**: `NOFILE` pequeno (não abre descritor novo útil), `NPROC` 0 (não cria
//!    processo nem thread), `FSIZE` 0 (não grava arquivo), `CORE` 0 (sem core dump com a
//!    memória do script) e `CPU` folgado (corta um filho que escapou do prazo do pai).
//!
//! Se um passo falha (kernel antigo, sem Landlock, ambiente que proíbe), **o filho serve
//! assim mesmo**: o isolamento principal é a ausência de `io`/`os`/rede na VM e o prazo do
//! pai; o SO é uma segunda camada. Nada vai para o stderr (que já é descartado pelo pai); o
//! resultado fica no [`RelatorioDeEndurecimento`], que o pai pode pedir ao filho
//! (`ExecutorFilho::endurecimento`) para o Rastro e para os testes.
//!
//! Esta função **não** é chamada por [`crate::servir`] (os testes rodam `servir` no próprio
//! processo de teste e não podem se trancar); é chamada pelo binário filho e por
//! [`crate::servir_endurecido`].

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Estado {
    /// Tudo o que se pediu foi aplicado.
    Total,
    /// Aplicado em parte (por exemplo, kernel sem a parte de rede do Landlock).
    Parcial,
    /// Não foi possível aplicar.
    Nenhum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatorioDeEndurecimento {
    /// Acesso a arquivos (e rede, quando a ABI deixa) negado pelo Landlock.
    pub landlock: Estado,
    /// `RLIMIT_NOFILE`, `NPROC`, `FSIZE`, `CORE` e `CPU`.
    pub rlimits: Estado,
}

/// Endurece o processo atual. Nunca falha: devolve o que conseguiu.
#[must_use]
pub fn endurecer() -> RelatorioDeEndurecimento {
    RelatorioDeEndurecimento {
        landlock: aplicar_landlock(),
        rlimits: aplicar_rlimits(),
    }
}

#[cfg(target_os = "linux")]
fn aplicar_landlock() -> Estado {
    use landlock::{
        ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, Ruleset, RulesetAttr,
        RulesetStatus,
    };
    let abi = ABI::V5;
    let resultado = (|| -> Result<RulesetStatus, landlock::RulesetError> {
        let estado = Ruleset::default()
            .set_compatibility(CompatLevel::BestEffort)
            .handle_access(AccessFs::from_all(abi))?
            .handle_access(AccessNet::from_all(abi))?
            .create()?
            .restrict_self()?;
        Ok(estado.ruleset)
    })();
    match resultado {
        Ok(RulesetStatus::FullyEnforced) => Estado::Total,
        Ok(RulesetStatus::PartiallyEnforced) => Estado::Parcial,
        Ok(RulesetStatus::NotEnforced) | Err(_) => Estado::Nenhum,
    }
}

#[cfg(not(target_os = "linux"))]
fn aplicar_landlock() -> Estado {
    Estado::Nenhum
}

#[cfg(unix)]
fn aplicar_rlimits() -> Estado {
    use rlimit::Resource;
    let pedidos = [
        (Resource::NOFILE, 8),
        (Resource::NPROC, 0),
        (Resource::FSIZE, 0),
        (Resource::CORE, 0),
        (Resource::CPU, 600),
    ];
    let aplicados = pedidos
        .iter()
        .filter(|(recurso, valor)| recurso.set(*valor, *valor).is_ok())
        .count();
    if aplicados == pedidos.len() {
        Estado::Total
    } else if aplicados == 0 {
        Estado::Nenhum
    } else {
        Estado::Parcial
    }
}

#[cfg(not(unix))]
fn aplicar_rlimits() -> Estado {
    Estado::Nenhum
}
