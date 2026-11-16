//! O que a janela lembra entre aberturas, fora do que a casca já guarda: o zoom (Ctrl + e
//! Ctrl −) e o jeito que o Controle mensal foi deixado (cartões, faixa de atrasadas, modo do
//! período, filtro e grupos recolhidos).
//!
//! Vai para um JSON na pasta de dados do app (nunca no cofre), ao lado das preferências das
//! Ferramentas. Arquivo ausente ou estragado vale como "nada guardado": a janela abre com o
//! padrão e não reclama.

use std::path::{Path, PathBuf};

use sbf_planejamento::Filtro;
use serde::{Deserialize, Serialize};

use crate::controle::ModoDoPeriodo;
use crate::controle_ui::AbaDoDetalhe;

/// O nome do arquivo, ao lado de `preferencias-das-ferramentas.json`.
pub const NOME_DO_ARQUIVO: &str = "preferencias-da-janela.json";

/// Zoom que a casca aceita (`definir_aparencia` limita a esta faixa).
const ZOOM_MINIMO: f32 = 0.5;
const ZOOM_MAXIMO: f32 = 3.0;

fn verdadeiro() -> bool {
    true
}

/// O jeito que o Controle mensal foi deixado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreferenciasDoControle {
    /// Os cartões do resumo (ligado por padrão).
    #[serde(default = "verdadeiro")]
    pub resumo_aberto: bool,
    /// A faixa "Em atraso de antes" aberta (por padrão, sim).
    #[serde(default = "verdadeiro")]
    pub atrasadas_abertas: bool,
    #[serde(default)]
    pub modo: ModoDoPeriodo,
    #[serde(default)]
    pub filtro: Filtro,
    /// As chaves dos grupos e subgrupos recolhidos, em ordem (para comparar).
    #[serde(default)]
    pub recolhidos: Vec<String>,
    /// A aba do Detalhe (valor desconhecido vale Resumo).
    #[serde(default)]
    pub aba_do_detalhe: AbaDoDetalhe,
}

impl Default for PreferenciasDoControle {
    fn default() -> Self {
        Self {
            resumo_aberto: true,
            atrasadas_abertas: true,
            modo: ModoDoPeriodo::default(),
            filtro: Filtro::default(),
            recolhidos: Vec::new(),
            aba_do_detalhe: AbaDoDetalhe::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PreferenciasDaJanela {
    /// O `zoom_factor` do egui; `None` enquanto nada foi guardado.
    #[serde(default)]
    pub zoom: Option<f32>,
    #[serde(default)]
    pub controle: PreferenciasDoControle,
    /// As preferências da casca (tema, densidade, modo da faixa, painéis visíveis...).
    #[serde(default)]
    pub casca: Option<bancada_casca::Preferencias>,
    /// A largura da lateral (dock) em pontos.
    #[serde(default)]
    pub largura_da_lateral: Option<f32>,
}

/// Largura que a casca aceita para a lateral (`size_range` do dock).
pub const LARGURA_MINIMA: f32 = 220.0;
pub const LARGURA_MAXIMA: f32 = 720.0;

/// Limita uma largura guardada à faixa da lateral; não finito não vale.
pub fn limitar_largura(l: f32) -> Option<f32> {
    l.is_finite()
        .then(|| l.clamp(LARGURA_MINIMA, LARGURA_MAXIMA))
}

impl PreferenciasDaJanela {
    /// O zoom guardado, se for um número que a casca aceita.
    pub fn zoom_valido(&self) -> Option<f32> {
        self.zoom
            .filter(|z| z.is_finite() && (ZOOM_MINIMO..=ZOOM_MAXIMO).contains(z))
    }

    /// A largura guardada, limitada à faixa da lateral.
    pub fn largura_valida(&self) -> Option<f32> {
        self.largura_da_lateral.and_then(limitar_largura)
    }

    /// As preferências da casca a reaplicar na abertura. O zoom guardado manda na escala
    /// (ele tem caminho próprio), para os dois não brigarem.
    pub fn casca_para_aplicar(&self) -> Option<bancada_casca::Preferencias> {
        let mut p = self.casca.clone()?;
        if let Some(z) = self.zoom_valido() {
            p.escala = z;
        }
        Some(p)
    }

    /// Lê o arquivo; sem arquivo ou com arquivo ruim, o padrão.
    pub fn carregar(arquivo: &Path) -> Self {
        std::fs::read_to_string(arquivo)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Grava por inteiro (escreve num temporário e renomeia, para não deixar meio arquivo).
    pub fn salvar(&self, arquivo: &Path) -> Result<(), String> {
        if let Some(pasta) = arquivo.parent() {
            std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
        }
        let texto = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let temporario = arquivo.with_extension("json.tmp");
        std::fs::write(&temporario, texto).map_err(|e| e.to_string())?;
        std::fs::rename(&temporario, arquivo).map_err(|e| e.to_string())
    }
}

/// Quem sabe onde o arquivo mora e o que foi gravado por último: só grava o que mudou.
#[derive(Debug, Default)]
pub struct Guardador {
    arquivo: Option<PathBuf>,
    ultimo: Option<PreferenciasDaJanela>,
    resolvido: bool,
}

impl Guardador {
    /// O arquivo já foi decidido (com ou sem pasta de dados).
    pub fn resolvido(&self) -> bool {
        self.resolvido
    }

    pub fn arquivo(&self) -> Option<&Path> {
        self.arquivo.as_deref()
    }

    /// Decide o arquivo (`None`: sem pasta de dados, nada é guardado) e devolve o que havia
    /// nele.
    pub fn ligar(&mut self, arquivo: Option<PathBuf>) -> PreferenciasDaJanela {
        self.resolvido = true;
        let lidas = arquivo
            .as_deref()
            .map(PreferenciasDaJanela::carregar)
            .unwrap_or_default();
        self.ultimo = Some(lidas.clone());
        self.arquivo = arquivo;
        lidas
    }

    /// Um quadro: grava se `atual` difere do último gravado. `Ok(true)` quando gravou.
    pub fn quadro(&mut self, atual: &PreferenciasDaJanela) -> Result<bool, String> {
        let Some(arquivo) = self.arquivo.as_deref() else {
            return Ok(false);
        };
        if self.ultimo.as_ref() == Some(atual) {
            return Ok(false);
        }
        // Mesmo se falhar, não tenta de novo a cada quadro com o mesmo conteúdo.
        self.ultimo = Some(atual.clone());
        atual.salvar(arquivo).map(|()| true)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn caminho() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().expect("pasta");
        let a = t.path().join("dados").join(NOME_DO_ARQUIVO);
        (t, a)
    }

    fn alteradas() -> PreferenciasDaJanela {
        PreferenciasDaJanela {
            zoom: Some(1.25),
            controle: PreferenciasDoControle {
                resumo_aberto: false,
                atrasadas_abertas: false,
                modo: ModoDoPeriodo::MesCivil,
                filtro: Filtro::EmAtraso,
                recolhidos: vec!["g:Lar".into(), "s:Lar:Moradia".into()],
                aba_do_detalhe: AbaDoDetalhe::Vigencia,
            },
            casca: Some(bancada_casca::Preferencias {
                tema: "escuro".into(),
                densidade: bancada_casca::Densidade::Compacta,
                escala: 1.25,
                modo_ribbon: bancada_casca::ModoRibbon::Simples,
                paineis_visiveis: [("previa".to_owned(), false)].into_iter().collect(),
                tela_cheia: false,
                modo_foco: true,
            }),
            largura_da_lateral: Some(410.0),
        }
    }

    #[test]
    fn ida_e_volta() {
        let (_t, a) = caminho();
        let p = alteradas();
        p.salvar(&a).unwrap();
        assert_eq!(PreferenciasDaJanela::carregar(&a), p);
    }

    #[test]
    fn sem_arquivo_ou_com_lixo_vale_o_padrao() {
        let (_t, a) = caminho();
        assert_eq!(
            PreferenciasDaJanela::carregar(&a),
            PreferenciasDaJanela::default()
        );
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::write(&a, "isto não é json").unwrap();
        assert_eq!(
            PreferenciasDaJanela::carregar(&a),
            PreferenciasDaJanela::default()
        );
        // O padrão abre com cartões e faixa ligados, no ciclo, sem filtro.
        let d = PreferenciasDaJanela::default().controle;
        assert!(d.resumo_aberto && d.atrasadas_abertas);
        assert_eq!(d.modo, ModoDoPeriodo::Ciclo);
        assert_eq!(d.filtro, Filtro::Todos);
    }

    #[test]
    fn arquivo_antigo_sem_campos_novos_ainda_abre() {
        let (_t, a) = caminho();
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::write(&a, r#"{"zoom": 1.5}"#).unwrap();
        let p = PreferenciasDaJanela::carregar(&a);
        assert_eq!(p.zoom, Some(1.5));
        assert!(p.controle.resumo_aberto);
    }

    #[test]
    fn zoom_fora_da_faixa_nao_vale() {
        for z in [0.1, 9.0, f32::NAN] {
            let p = PreferenciasDaJanela {
                zoom: Some(z),
                ..Default::default()
            };
            assert_eq!(p.zoom_valido(), None);
        }
        let p = PreferenciasDaJanela {
            zoom: Some(1.1),
            ..Default::default()
        };
        assert_eq!(p.zoom_valido(), Some(1.1));
    }

    #[test]
    fn guardador_so_grava_quando_muda() {
        let (_t, a) = caminho();
        let mut g = Guardador::default();
        assert!(!g.resolvido());
        assert_eq!(g.ligar(Some(a.clone())), PreferenciasDaJanela::default());
        assert!(g.resolvido());
        // Igual ao que havia: não grava (nem cria o arquivo).
        assert_eq!(g.quadro(&PreferenciasDaJanela::default()), Ok(false));
        assert!(!a.exists());
        let p = alteradas();
        assert_eq!(g.quadro(&p), Ok(true));
        assert_eq!(g.quadro(&p), Ok(false));
        // Uma nova abertura lê o que foi gravado.
        let mut outro = Guardador::default();
        assert_eq!(outro.ligar(Some(a)), p);
    }

    #[test]
    fn sem_pasta_de_dados_nada_e_guardado() {
        let mut g = Guardador::default();
        g.ligar(None);
        assert_eq!(g.quadro(&alteradas()), Ok(false));
    }

    #[test]
    fn casca_e_largura_vao_e_voltam_e_arquivo_antigo_abre() {
        let (_t, a) = caminho();
        let p = alteradas();
        p.salvar(&a).unwrap();
        let lidas = PreferenciasDaJanela::carregar(&a);
        assert_eq!(lidas.casca, p.casca);
        assert_eq!(lidas.largura_da_lateral, Some(410.0));
        assert_eq!(lidas.controle.aba_do_detalhe, AbaDoDetalhe::Vigencia);
        std::fs::write(&a, r#"{"zoom": 1.5, "controle": {"resumo_aberto": false}}"#).unwrap();
        let antigo = PreferenciasDaJanela::carregar(&a);
        assert_eq!(antigo.zoom, Some(1.5));
        assert!(antigo.casca.is_none() && antigo.largura_da_lateral.is_none());
        assert_eq!(antigo.controle.aba_do_detalhe, AbaDoDetalhe::Resumo);
    }

    #[test]
    fn restauracao_entrega_a_casca_com_o_zoom_guardado_na_escala() {
        let mut p = alteradas();
        p.zoom = Some(1.5);
        let c = p.casca_para_aplicar().expect("casca");
        assert_eq!(c.escala, 1.5);
        assert!(c.modo_foco);
        p.zoom = None;
        assert_eq!(p.casca_para_aplicar().unwrap().escala, 1.25);
        p.casca = None;
        assert!(p.casca_para_aplicar().is_none());
    }

    #[test]
    fn aba_desconhecida_vale_resumo() {
        let c: PreferenciasDoControle =
            serde_json::from_str(r#"{"aba_do_detalhe": "Proximas"}"#).unwrap();
        assert_eq!(c.aba_do_detalhe, AbaDoDetalhe::Resumo);
    }

    #[test]
    fn largura_e_limitada_a_faixa_da_lateral() {
        assert_eq!(limitar_largura(100.0), Some(220.0));
        assert_eq!(limitar_largura(5000.0), Some(720.0));
        assert_eq!(limitar_largura(340.0), Some(340.0));
        assert_eq!(limitar_largura(f32::NAN), None);
        assert_eq!(limitar_largura(f32::INFINITY), None);
        let p = PreferenciasDaJanela {
            largura_da_lateral: Some(f32::NAN),
            ..Default::default()
        };
        assert_eq!(p.largura_valida(), None);
    }
}
