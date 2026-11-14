//! Os roteiros de verdade no `sbf worker` (spec 10, C1): executa os agendados com o Chrome e o
//! chaveiro do usuário e, se um passo falha, pede à IA (o `claude` do usuário, via o mesmo motor
//! da triagem) um conserto que **fica como proposta** até o usuário aprovar na janela.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use sbf_chaveiro::{Chaveiro, Segredo};
use sbf_ia::{NivelDeConfianca, Registro};
use sbf_roteiros::executor::FonteDeSenhas;
use sbf_roteiros::gravacao::Guia;
use sbf_roteiros::servico::{Ambiente, executar_devidos};
use sbf_worker::laco::{ColetorDeRoteiros, Dados, ResumoDeRoteiros};
use sbf_worker::registro::Falha;

use crate::triagem::{Insumos, montar_motor};

/// Sem chaveiro aberto: os passos que pedem senha falham com um motivo claro.
struct SemChaveiro;

impl FonteDeSenhas for SemChaveiro {
    fn senha_de_site(&self, _rotulo: &str) -> Result<Segredo, String> {
        Err("o chaveiro não abriu sem a janela".to_owned())
    }
    fn cpf(&self) -> Result<Segredo, String> {
        Err("o chaveiro não abriu sem a janela".to_owned())
    }
}

/// Quanto o worker espera o usuário numa pausa: ele pode nem estar no computador.
const PAUSA_NO_WORKER: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// O ambiente dos roteiros para um cofre e uma pasta de dados.
#[must_use]
pub fn ambiente(cofre: &Path, pasta_de_dados: &Path) -> Ambiente {
    Ambiente::novo(pasta_de_dados, cofre)
}

/// O executor de roteiros agendados do worker.
pub struct RoteirosReais {
    dados: Dados,
}

impl RoteirosReais {
    #[must_use]
    pub fn novo(dados: Dados) -> Self {
        Self { dados }
    }
}

impl ColetorDeRoteiros for RoteirosReais {
    fn executar_devidos(
        &mut self,
        agora_: u64,
        cancelar: &AtomicBool,
    ) -> Result<ResumoDeRoteiros, Falha> {
        let mut amb = ambiente(&self.dados.cofre, &self.dados.pasta);
        amb.prazo_da_pausa = PAUSA_NO_WORKER;
        let banco = amb.banco().map_err(|_| Falha::Banco)?;
        let agora_i = i64::try_from(agora_).unwrap_or(i64::MAX);
        if banco.devidos(agora_i).map_err(|_| Falha::Banco)?.is_empty() {
            return Ok(ResumoDeRoteiros::default());
        }
        // O worker não pede senha: se o chaveiro não abre sozinho, os roteiros com senha falham.
        let chaveiro = Chaveiro::abrir_na_pasta_padrao().ok();
        let senhas: &dyn FonteDeSenhas = match &chaveiro {
            Some(c) => c,
            None => &SemChaveiro,
        };
        let insumos = Insumos::do_ambiente(chaveiro.clone());
        let motor = montar_motor(&self.dados.banco, &insumos, true).ok();
        let registro: Option<&dyn Registro> = motor.as_ref().map(|m| &*m.registro as &dyn Registro);
        let guia = motor.as_ref().map(|m| Guia {
            provedor: &*m.principal,
            registro,
            destinatario: "roteiros".to_owned(),
            nivel: NivelDeConfianca::Mascarado,
            rotulos_de_senha: chaveiro
                .as_ref()
                .map(Chaveiro::rotulos_de_site)
                .unwrap_or_default(),
            tem_cpf: chaveiro.as_ref().is_some_and(Chaveiro::tem_cpf),
            segredos: Vec::new(),
        });
        let r = executar_devidos(&amb, agora_i, senhas, guia.as_ref(), cancelar)
            .map_err(|_| Falha::Externo)?;
        Ok(ResumoDeRoteiros {
            executados: r.executados,
            baixados: r.baixados as usize,
            novos: r.novos as usize,
        })
    }
}

/// A IA dos roteiros na janela: o mesmo motor da triagem (o `claude` do usuário como
/// principal), montado quando a gravação ou o conserto pedem.
pub struct IaDoInventario {
    pub banco: std::path::PathBuf,
    pub insumos: Insumos,
}

impl sbf_janela::roteiros::IaDosRoteiros for IaDoInventario {
    fn pronta(&self) -> Result<sbf_janela::roteiros::ProvedorEmRegistro, String> {
        let motor = montar_motor(&self.dados.banco, &insumos, true).ok();
        let registro: Option<&dyn Registro> = motor.as_ref().map(|m| &*m.registro as &dyn Registro);
        let guia = motor.as_ref().map(|m| Guia {
            provedor: &*m.principal,
            registro,
            destinatario: "roteiros".to_owned(),
            nivel: NivelDeConfianca::Mascarado,
            rotulos_de_senha: chaveiro
                .as_ref()
                .map(Chaveiro::rotulos_de_site)
                .unwrap_or_default(),
            tem_cpf: chaveiro.as_ref().is_some_and(Chaveiro::tem_cpf),
            segredos: Vec::new(),
        });
        let r = executar_devidos(&amb, agora_i, senhas, guia.as_ref(), cancelar)
            .map_err(|_| Falha::Externo)?;
        Ok(ResumoDeRoteiros {
            executados: r.executados,
            baixados: r.baixados as usize,
            novos: r.novos as usize,
        })
    }
}

/// A IA dos roteiros na janela: o mesmo motor da triagem (o `claude` do usuário como
/// principal), montado quando a gravação ou o conserto pedem.
pub struct IaDoInventario {
    pub banco: std::path::PathBuf,
    pub insumos: Insumos,
}

impl sbf_janela::roteiros::IaDosRoteiros for IaDoInventario {
    fn pronta(
        &self,
    ) -> Result<
        (
            std::sync::Arc<dyn sbf_ia::Provedor>,
            std::sync::Arc<dyn Registro>,
        ),
        String,
    > {
        let motor = montar_motor(&self.banco, &self.insumos, true)?;
        Ok((motor.principal, motor.registro))
    }
}
