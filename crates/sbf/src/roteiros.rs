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

/// Roda um trabalho demorado numa thread própria e devolve o resultado na passada seguinte:
/// um roteiro que espera o usuário (MFA, até 10 minutos) não pode segurar o laço do worker,
/// que tem coleta, triagem e índice para tocar.
pub struct ExecucaoEmFio {
    fio: Option<std::thread::JoinHandle<Result<ResumoDeRoteiros, Falha>>>,
    cancelar: std::sync::Arc<AtomicBool>,
}

impl ExecucaoEmFio {
    #[must_use]
    pub fn nova() -> Self {
        Self {
            fio: None,
            cancelar: std::sync::Arc::new(AtomicBool::new(false)),
        }
    }

    /// Há um trabalho em andamento?
    #[must_use]
    pub fn rodando(&self) -> bool {
        self.fio.as_ref().is_some_and(|f| !f.is_finished())
    }

    /// Uma passada do laço. Nunca bloqueia: recolhe o resultado de um trabalho que terminou
    /// e, se nenhum está em andamento, inicia `trabalho` numa thread (o `cancelar` do
    /// trabalho acende quando o do worker acende). Devolve o resumo recolhido, ou vazio.
    ///
    /// # Errors
    /// A falha do trabalho que terminou.
    pub fn passar(
        &mut self,
        cancelar: &AtomicBool,
        trabalho: impl FnOnce(std::sync::Arc<AtomicBool>) -> Result<ResumoDeRoteiros, Falha>
        + Send
        + 'static,
    ) -> Result<ResumoDeRoteiros, Falha> {
        if cancelar.load(std::sync::atomic::Ordering::Relaxed) {
            self.cancelar
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let mut recolhido = Ok(ResumoDeRoteiros::default());
        if let Some(f) = self.fio.take() {
            if f.is_finished() {
                recolhido = f.join().unwrap_or(Err(Falha::Externo));
            } else {
                self.fio = Some(f);
                return recolhido;
            }
        }
        if !self.cancelar.load(std::sync::atomic::Ordering::Relaxed) {
            let c = std::sync::Arc::clone(&self.cancelar);
            self.fio = Some(std::thread::spawn(move || trabalho(c)));
        }
        recolhido
    }
}

impl Default for ExecucaoEmFio {
    fn default() -> Self {
        Self::nova()
    }
}

impl Drop for ExecucaoEmFio {
    fn drop(&mut self) {
        // o worker saiu: o roteiro em curso é cancelado (a pausa e os passos olham o sinal)
        self.cancelar
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// O executor de roteiros agendados do worker. O roteiro roda numa thread própria: o laço
/// não espera MFA nem CAPTCHA.
pub struct RoteirosReais {
    dados: Dados,
    em_fio: ExecucaoEmFio,
}

impl RoteirosReais {
    #[must_use]
    pub fn novo(dados: Dados) -> Self {
        Self {
            dados,
            em_fio: ExecucaoEmFio::nova(),
        }
    }
}

impl ColetorDeRoteiros for RoteirosReais {
    fn executar_devidos(
        &mut self,
        agora_: u64,
        cancelar: &AtomicBool,
    ) -> Result<ResumoDeRoteiros, Falha> {
        if self.em_fio.rodando() {
            return self
                .em_fio
                .passar(cancelar, |_| Ok(ResumoDeRoteiros::default()));
        }
        // Só abre o chaveiro e o motor se há o que rodar.
        let mut amb = ambiente(&self.dados.cofre, &self.dados.pasta);
        amb.prazo_da_pausa = PAUSA_NO_WORKER;
        let banco = amb.banco().map_err(|_| Falha::Banco)?;
        let agora_i = i64::try_from(agora_).unwrap_or(i64::MAX);
        if banco.devidos(agora_i).map_err(|_| Falha::Banco)?.is_empty() {
            return self
                .em_fio
                .passar(cancelar, |_| Ok(ResumoDeRoteiros::default()));
        }
        let dados = self.dados.clone();
        self.em_fio.passar(cancelar, move |cancelar| {
            rodar_devidos(&dados, &amb, agora_i, &cancelar)
        })
    }
}

/// O trabalho da thread: chaveiro, IA e a passada dos roteiros devidos.
fn rodar_devidos(
    dados: &Dados,
    amb: &Ambiente,
    agora_i: i64,
    cancelar: &AtomicBool,
) -> Result<ResumoDeRoteiros, Falha> {
    // O worker não pede senha: se o chaveiro não abre sozinho, os roteiros com senha falham.
    let chaveiro = Chaveiro::abrir_na_pasta_padrao().ok();
    let senhas: &dyn FonteDeSenhas = match &chaveiro {
        Some(c) => c,
        None => &SemChaveiro,
    };
    let insumos = Insumos::do_ambiente(chaveiro.clone());
    let motor = montar_motor(&dados.banco, &insumos, true).ok();
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
    let r = executar_devidos(amb, agora_i, senhas, guia.as_ref(), cancelar)
        .map_err(|_| Falha::Externo)?;
    Ok(ResumoDeRoteiros {
        executados: r.executados,
        baixados: r.baixados as usize,
        novos: r.novos as usize,
    })
}

/// A IA dos roteiros na janela: o mesmo motor da triagem (o `claude` do usuário como
/// principal), montado quando a gravação ou o conserto pedem.
pub struct IaDoInventario {
    pub banco: std::path::PathBuf,
    pub insumos: Insumos,
}

impl sbf_janela::roteiros::IaDosRoteiros for IaDoInventario {
    fn pronta(&self) -> Result<sbf_janela::roteiros::ProvedorEmRegistro, String> {
        let motor = montar_motor(&self.banco, &self.insumos, true)?;
        Ok((motor.principal, motor.registro))
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn o_roteiro_lento_nao_trava_o_laco_e_o_resultado_chega_na_passada_seguinte() {
        let nao = AtomicBool::new(false);
        let mut f = ExecucaoEmFio::nova();
        let t0 = Instant::now();
        let r = f.passar(&nao, |_| {
            std::thread::sleep(Duration::from_millis(400));
            Ok(ResumoDeRoteiros {
                executados: 1,
                baixados: 2,
                novos: 1,
            })
        });
        assert!(
            t0.elapsed() < Duration::from_millis(200),
            "a passada não espera"
        );
        assert_eq!(r.unwrap(), ResumoDeRoteiros::default());
        assert!(f.rodando());
        // enquanto roda, outra passada não inicia nada nem bloqueia
        let t1 = Instant::now();
        let r = f.passar(&nao, |_| panic!("não podia iniciar de novo"));
        assert!(t1.elapsed() < Duration::from_millis(200));
        assert_eq!(r.unwrap(), ResumoDeRoteiros::default());
        std::thread::sleep(Duration::from_millis(600));
        let r = f.passar(&nao, |_| Ok(ResumoDeRoteiros::default())).unwrap();
        assert_eq!((r.executados, r.baixados, r.novos), (1, 2, 1));
    }

    #[test]
    fn cancelar_o_worker_cancela_o_roteiro_em_curso() {
        let nao = AtomicBool::new(false);
        let mut f = ExecucaoEmFio::nova();
        let _ = f.passar(&nao, |c| {
            while !c.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(ResumoDeRoteiros::default())
        });
        let sim = AtomicBool::new(true);
        let _ = f.passar(&sim, |_| Ok(ResumoDeRoteiros::default()));
        let limite = Instant::now() + Duration::from_secs(2);
        while f.rodando() && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!f.rodando());
    }
}
