//! O que a janela lembra entre aberturas, fora do que a casca já guarda: o zoom (Ctrl + e
//! Ctrl −) e o jeito que o Controle mensal foi deixado (cartões, faixa de atrasadas, modo do
//! período, filtro e grupos recolhidos).
//!
//! Vai para um JSON na pasta de dados do app (nunca no cofre), ao lado das preferências das
//! Ferramentas. Arquivo ausente ou estragado vale como "nada guardado": a janela abre com o
//! padrão e não reclama.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

/// Menor e maior tamanho (em pontos) que se aceita de um arquivo guardado.
pub const TAMANHO_MINIMO: [f32; 2] = [960.0, 600.0];
pub const TAMANHO_MAXIMO: [f32; 2] = [16_000.0, 16_000.0];
/// Posição externa (em pontos) fora desta faixa em cada eixo não vale (monitor que sumiu).
pub const POSICAO_LIMITE: f32 = 16_000.0;

/// O que a janela do sistema mostra num quadro (lido de `ViewportInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Observacao {
    pub maximizada: bool,
    pub tela_cheia: bool,
    pub minimizada: bool,
    /// Tamanho interno, em pontos.
    pub tamanho: Option<[f32; 2]>,
    /// Posição externa, em pontos (só o X11 e os sistemas que a informam entregam).
    pub posicao: Option<[f32; 2]>,
}

/// Como a janela do sistema foi deixada: maximizada ou não, o tamanho "normal" e a posição.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EstadoDaJanela {
    #[serde(default)]
    pub maximizada: bool,
    /// Tamanho interno quando não maximizada.
    #[serde(default)]
    pub tamanho: Option<[f32; 2]>,
    /// Posição externa quando não maximizada (só vale onde o sistema deixa pôr a posição).
    #[serde(default)]
    pub posicao: Option<[f32; 2]>,
}

impl EstadoDaJanela {
    /// O tamanho guardado, limitado a uma faixa sã; não finito não vale.
    pub fn tamanho_valido(&self) -> Option<[f32; 2]> {
        let [l, a] = self.tamanho?;
        (l.is_finite() && a.is_finite()).then(|| {
            [
                l.clamp(TAMANHO_MINIMO[0], TAMANHO_MAXIMO[0]),
                a.clamp(TAMANHO_MINIMO[1], TAMANHO_MAXIMO[1]),
            ]
        })
    }

    /// A posição guardada, se couber na faixa sã.
    pub fn posicao_valida(&self) -> Option<[f32; 2]> {
        let [x, y] = self.posicao?;
        let ok = |v: f32| v.is_finite() && v.abs() <= POSICAO_LIMITE;
        (ok(x) && ok(y)).then_some([x, y])
    }

    /// O estado depois de ver a janela neste quadro: tela cheia e minimizada não contam, e
    /// maximizada não sobrescreve o tamanho "normal".
    pub fn depois_de(&self, obs: &Observacao) -> Self {
        if obs.tela_cheia || obs.minimizada {
            return *self;
        }
        let mut novo = *self;
        novo.maximizada = obs.maximizada;
        if !obs.maximizada {
            let visto = Self {
                tamanho: obs.tamanho,
                posicao: obs.posicao,
                ..Default::default()
            };
            if let Some(t) = visto.tamanho_valido() {
                novo.tamanho = Some(t);
            }
            if let Some(p) = visto.posicao_valida() {
                novo.posicao = Some(p);
            }
        }
        novo
    }

    /// Aplica o estado guardado ao `ViewportBuilder` da abertura. A posição só entra quando
    /// `com_posicao` (no Wayland o sistema não deixa o app escolher).
    pub fn aplicar(
        &self,
        mut vb: egui::ViewportBuilder,
        com_posicao: bool,
    ) -> egui::ViewportBuilder {
        if let Some([l, a]) = self.tamanho_valido() {
            vb = vb.with_inner_size([l, a]);
        }
        if com_posicao && let Some([x, y]) = self.posicao_valida() {
            vb = vb.with_position([x, y]);
        }
        if self.maximizada {
            vb = vb.with_maximized(true);
        }
        vb
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
    /// A janela do sistema: maximizada, tamanho e posição.
    #[serde(default)]
    pub janela: EstadoDaJanela,
}

/// Onde moram as preferências da janela: na pasta de dados do app, nunca dentro do cofre.
/// `None` sem pasta de dados ou com ela dentro do cofre.
pub fn arquivo_das_preferencias_do(cofre: Option<&Path>) -> Option<PathBuf> {
    let pasta = sbf_chaveiro::pasta_de_dados_do_app().ok()?;
    if let Some(c) = cofre {
        sbf_chaveiro::garantir_fora_do_cofre(&pasta, c).ok()?;
    }
    Some(pasta.join(NOME_DO_ARQUIVO))
}

/// A espera do Guardador: o que muda a cada quadro (arrastar divisão, mover janela) só vai
/// para o disco quando para de mudar por este tempo.
pub const ESPERA_DE_GRAVACAO: Duration = Duration::from_millis(800);

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
    /// O que está esperando para ser gravado e desde quando não muda.
    pendente: Option<(PreferenciasDaJanela, Instant)>,
    botao_antes: bool,
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

    /// Há algo mudado esperando a hora de gravar (o app pede um quadro para depois).
    pub fn tem_pendente(&self) -> bool {
        self.pendente.is_some()
    }

    /// Um quadro com espera: grava `atual` quando ele para de mudar por `ESPERA_DE_GRAVACAO`
    /// ou quando o botão do mouse acaba de soltar (fim de um arrasto). `Ok(true)` quando
    /// gravou.
    pub fn quadro_com_espera(
        &mut self,
        atual: &PreferenciasDaJanela,
        agora: Instant,
        botao_apertado: bool,
    ) -> Result<bool, String> {
        let soltou = self.botao_antes && !botao_apertado;
        self.botao_antes = botao_apertado;
        if self.arquivo.is_none() || self.ultimo.as_ref() == Some(atual) {
            self.pendente = None;
            return Ok(false);
        }
        let desde = match &self.pendente {
            Some((p, desde)) if p == atual => *desde,
            _ => {
                self.pendente = Some((atual.clone(), agora));
                agora
            }
        };
        if botao_apertado {
            return Ok(false);
        }
        if soltou || agora.saturating_duration_since(desde) >= ESPERA_DE_GRAVACAO {
            self.pendente = None;
            return self.quadro(atual);
        }
        Ok(false)
    }

    /// Grava agora o que estava esperando (o app vai fechar). `Ok(true)` quando gravou.
    pub fn descarregar(&mut self) -> Result<bool, String> {
        match self.pendente.take() {
            Some((p, _)) => self.quadro(&p),
            None => Ok(false),
        }
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

    #[test]
    fn descarregar_grava_o_pendente_antes_da_espera() {
        let (_d, arquivo) = caminho();
        let mut g = Guardador::default();
        g.ligar(Some(arquivo.clone()));
        let p = PreferenciasDaJanela {
            zoom: Some(1.25),
            ..Default::default()
        };
        let agora = Instant::now();
        assert_eq!(g.quadro_com_espera(&p, agora, false), Ok(false));
        assert!(g.tem_pendente());
        assert_eq!(g.descarregar(), Ok(true));
        assert!(!g.tem_pendente());
        assert_eq!(PreferenciasDaJanela::carregar(&arquivo).zoom, Some(1.25));
        assert_eq!(g.descarregar(), Ok(false));
    }

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
                dock: None,
            }),
            largura_da_lateral: Some(410.0),
            janela: EstadoDaJanela {
                maximizada: true,
                tamanho: Some([1400.0, 900.0]),
                posicao: Some([120.0, 40.0]),
            },
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

    fn obs(maximizada: bool, t: [f32; 2], p: [f32; 2]) -> Observacao {
        Observacao {
            maximizada,
            tamanho: Some(t),
            posicao: Some(p),
            ..Default::default()
        }
    }

    #[test]
    fn arquivo_sem_o_campo_da_janela_abre_nao_maximizado() {
        let (_t, a) = caminho();
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::write(&a, r#"{"zoom": 1.5, "largura_da_lateral": 400.0}"#).unwrap();
        let p = PreferenciasDaJanela::carregar(&a);
        assert_eq!(p.janela, EstadoDaJanela::default());
        assert!(!p.janela.maximizada);
    }

    #[test]
    fn tamanho_e_posicao_ficam_em_limites_sanos() {
        let e = EstadoDaJanela {
            tamanho: Some([100.0, 99_999.0]),
            posicao: Some([30.0, 20.0]),
            ..Default::default()
        };
        assert_eq!(e.tamanho_valido(), Some([960.0, 16_000.0]));
        assert_eq!(e.posicao_valida(), Some([30.0, 20.0]));
        for ruim in [
            [f32::NAN, 1.0],
            [1.0, f32::INFINITY],
            [-90_000.0, 0.0],
            [0.0, 70_000.0],
        ] {
            let e = EstadoDaJanela {
                tamanho: Some(ruim),
                posicao: Some(ruim),
                ..Default::default()
            };
            assert_eq!(e.posicao_valida(), None, "{ruim:?}");
            if !ruim[0].is_finite() || !ruim[1].is_finite() {
                assert_eq!(e.tamanho_valido(), None, "{ruim:?}");
            }
        }
    }

    #[test]
    fn maximizada_nao_sobrescreve_o_tamanho_normal() {
        let normal =
            EstadoDaJanela::default().depois_de(&obs(false, [1300.0, 800.0], [10.0, 20.0]));
        assert_eq!(normal.tamanho, Some([1300.0, 800.0]));
        assert_eq!(normal.posicao, Some([10.0, 20.0]));
        let max = normal.depois_de(&obs(true, [1920.0, 1000.0], [0.0, 0.0]));
        assert!(max.maximizada);
        assert_eq!(max.tamanho, Some([1300.0, 800.0]));
        assert_eq!(max.posicao, Some([10.0, 20.0]));
        // Desmaximizar volta a seguir a janela.
        let de_volta = max.depois_de(&obs(false, [1200.0, 700.0 + 0.0], [5.0, 5.0]));
        assert!(!de_volta.maximizada);
        assert_eq!(de_volta.tamanho, Some([1200.0, 700.0]));
    }

    #[test]
    fn tela_cheia_e_minimizada_sao_ignoradas() {
        let antes = EstadoDaJanela::default().depois_de(&obs(false, [1300.0, 800.0], [10.0, 20.0]));
        for (tela_cheia, minimizada) in [(true, false), (false, true)] {
            let o = Observacao {
                tela_cheia,
                minimizada,
                maximizada: true,
                tamanho: Some([1920.0, 1080.0]),
                posicao: Some([0.0, 0.0]),
            };
            assert_eq!(antes.depois_de(&o), antes);
        }
    }

    #[test]
    fn sem_posicao_informada_a_guardada_fica() {
        let antes = EstadoDaJanela::default().depois_de(&obs(false, [1300.0, 800.0], [10.0, 20.0]));
        let wayland = Observacao {
            tamanho: Some([1310.0, 810.0]),
            posicao: None,
            ..Default::default()
        };
        let depois = antes.depois_de(&wayland);
        assert_eq!(depois.tamanho, Some([1310.0, 810.0]));
        assert_eq!(depois.posicao, Some([10.0, 20.0]));
    }

    #[test]
    fn aplicar_no_viewport_respeita_a_posicao_confiavel() {
        let e = alteradas().janela;
        let x11 = e.aplicar(egui::ViewportBuilder::default(), true);
        assert_eq!(x11.maximized, Some(true));
        assert_eq!(x11.inner_size, Some(egui::vec2(1400.0, 900.0)));
        assert_eq!(x11.position, Some(egui::pos2(120.0, 40.0)));
        let wayland = e.aplicar(egui::ViewportBuilder::default(), false);
        assert_eq!(wayland.position, None);
        let nada = EstadoDaJanela::default().aplicar(egui::ViewportBuilder::default(), true);
        assert_eq!(nada.maximized, None);
        assert_eq!(nada.inner_size, None);
    }

    fn com_arquivo() -> (tempfile::TempDir, Guardador) {
        let (t, a) = caminho();
        let mut g = Guardador::default();
        g.ligar(Some(a));
        (t, g)
    }

    #[test]
    fn guardador_espera_o_arrasto_parar() {
        let (_t, mut g) = com_arquivo();
        let t0 = Instant::now();
        let mut p = PreferenciasDaJanela::default();
        // Muda a cada quadro por meio segundo: nada vai ao disco.
        for ms in 0..5u64 {
            p.largura_da_lateral = Some(300.0 + ms as f32);
            let r = g.quadro_com_espera(&p, t0 + Duration::from_millis(ms * 100), false);
            assert_eq!(r, Ok(false));
        }
        assert!(g.tem_pendente());
        assert!(!g.arquivo().unwrap().exists());
        // Parou de mudar por mais que a espera: grava.
        let r = g.quadro_com_espera(&p, t0 + Duration::from_millis(400 + 900), false);
        assert_eq!(r, Ok(true));
        assert!(!g.tem_pendente());
        assert_eq!(PreferenciasDaJanela::carregar(g.arquivo().unwrap()), p);
    }

    #[test]
    fn guardador_grava_ao_soltar_o_botao_e_nao_com_ele_apertado() {
        let (_t, mut g) = com_arquivo();
        let t0 = Instant::now();
        let p = PreferenciasDaJanela {
            largura_da_lateral: Some(350.0),
            ..Default::default()
        };
        assert_eq!(g.quadro_com_espera(&p, t0, true), Ok(false));
        // Muito tempo depois, ainda com o botão apertado: espera.
        let tarde = t0 + Duration::from_secs(5);
        assert_eq!(g.quadro_com_espera(&p, tarde, true), Ok(false));
        assert_eq!(g.quadro_com_espera(&p, tarde, false), Ok(true));
    }

    #[test]
    fn guardador_sem_mudanca_nao_fica_pendente() {
        let (_t, mut g) = com_arquivo();
        let p = PreferenciasDaJanela::default();
        assert_eq!(g.quadro_com_espera(&p, Instant::now(), false), Ok(false));
        assert!(!g.tem_pendente());
    }
}
