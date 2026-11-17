//! A costura da área **Regras de reconhecimento** (spec 13, D12): o que a janela precisa do
//! catálogo (`regras.sqlite`) e do executor de scripts, sem egui. Toda ação que muda o
//! catálogo sai daqui com [`Quem::Usuario`]: o MCP e a IA nunca ativam, consolidam nem
//! revertem (D14), e o catálogo recusa qualquer outro `Quem`.
//!
//! O integrador liga ao cofre real com
//! `janela.regras.ligar(FonteDasRegras::nova(Arc::new(Mutex::new(Catalogo::abrir(cofre/regras.sqlite)?)), Some(executor)))`.
//! A demonstração e os testes usam [`FonteDasRegras::ficticia`], em memória.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use sbf_regras::catalogo::{
    Catalogo, Esperado, ExemploDeRegra, NovaVersao, RelatorioDosExemplos, ResumoDaRegra,
    VersaoDeRegra,
};
use sbf_regras::definicao::{
    Condicao, Definicao, Extracao, FonteDaExtracao, Formato, ModeloDeProposta,
};
use sbf_regras::motor::Motor;
use sbf_regras::script::ExecutorDeScript;
use sbf_regras::simulacao::{Diferencas, simular};
use sbf_regras::{
    Autor, Canal, Decisao, Dicas, Entrada, ErroDasRegras, Estado, IdDaRegra, Origem, Quem, Retorno,
    TipoDeEntrada, VERSAO_DO_ESQUEMA, Valor,
};
use sha2::{Digest, Sha256};

/// As entradas recentes que a simulação também lê (o integrador entrega do inventário).
pub type EntradasRecentes = Arc<dyn Fn() -> Vec<Entrada> + Send + Sync>;

/// O catálogo, o executor e as entradas recentes. Barato de clonar.
#[derive(Clone)]
pub struct FonteDasRegras {
    pub catalogo: Arc<Mutex<Catalogo>>,
    /// Verifica scripts no editor e roda os exemplos de uma regra com script. Sem ele, o
    /// editor avisa que o script não foi verificado e a ativação de regra com script bloqueia.
    pub executor: Option<Arc<dyn ExecutorDeScript>>,
    pub recentes: Option<EntradasRecentes>,
}

impl std::fmt::Debug for FonteDasRegras {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FonteDasRegras")
            .field("executor", &self.executor.is_some())
            .finish_non_exhaustive()
    }
}

/// Uma linha do Rastro da regra (histórico de estado de qualquer versão).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDoRastro {
    pub versao: u32,
    pub de: Option<String>,
    pub para: String,
    pub quem: String,
    pub motivo: String,
    pub em: String,
}

/// O resultado da simulação de uma versão contra os exemplos e as entradas recentes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulacaoDaRegra {
    pub versao: u32,
    pub total: usize,
    pub diferencas: Diferencas,
    /// id da entrada → rótulo para mostrar (nome do exemplo ou "entrada recente"). Nunca o
    /// texto do documento.
    pub rotulos: BTreeMap<String, String>,
}

fn erro(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Id estável de uma entrada colada: o sha256 do texto.
pub fn id_do_texto(texto: &str) -> String {
    let mut s = String::with_capacity(64);
    for b in Sha256::digest(texto.as_bytes()) {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// A entrada de um exemplo colado na janela: um documento digitado.
pub fn entrada_de_texto(texto: &str) -> Entrada {
    Entrada {
        versao_do_esquema: VERSAO_DO_ESQUEMA,
        id: id_do_texto(texto),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Manual,
            id_externo: None,
        },
        sha256: None,
        mime: Some("text/plain".to_owned()),
        nome: None,
        texto: Some(texto.to_owned()),
        dicas: Dicas::default(),
        momento: None,
    }
}

/// Lê o JSON de uma definição e valida (regexes, nomes). A mensagem diz o que está errado.
///
/// # Errors
/// O texto do erro de JSON ou de validação.
pub fn validar_definicao(json: &str) -> Result<Definicao, String> {
    let d: Definicao = serde_json::from_str(json).map_err(|e| format!("JSON inválido: {e}"))?;
    d.validar().map_err(erro)?;
    Ok(d)
}

/// O JSON bonito de uma definição, para o editor.
pub fn definicao_em_texto(d: &Definicao) -> String {
    serde_json::to_string_pretty(d).unwrap_or_default()
}

impl FonteDasRegras {
    pub fn nova(
        catalogo: Arc<Mutex<Catalogo>>,
        executor: Option<Arc<dyn ExecutorDeScript>>,
    ) -> Self {
        Self {
            catalogo,
            executor,
            recentes: None,
        }
    }

    #[must_use]
    pub fn com_recentes(mut self, recentes: EntradasRecentes) -> Self {
        self.recentes = Some(recentes);
        self
    }

    fn catalogo(&self) -> Result<std::sync::MutexGuard<'_, Catalogo>, String> {
        self.catalogo
            .lock()
            .map_err(|_| "o catálogo de regras travou; reabra a janela".to_owned())
    }

    fn executor_ref(&self) -> Option<&dyn ExecutorDeScript> {
        self.executor.as_deref()
    }

    // ---- leitura ----

    /// A lista de regras.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn resumos(&self) -> Result<Vec<ResumoDaRegra>, String> {
        self.catalogo()?.regras().map_err(erro)
    }

    /// As regras com versão ativa que acertaram o bastante seguidas (D4): `(regra, versão)`.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn sugestoes_de_consolidacao(&self) -> Result<Vec<(IdDaRegra, u32)>, String> {
        self.catalogo()?.sugestoes_de_consolidacao().map_err(erro)
    }

    /// As versões da regra, da primeira à última.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn versoes(&self, regra: &str) -> Result<Vec<VersaoDeRegra>, String> {
        self.catalogo()?.versoes(regra).map_err(erro)
    }

    /// Os exemplos da regra.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn exemplos(&self, regra: &str) -> Result<Vec<ExemploDeRegra>, String> {
        self.catalogo()?.exemplos(regra).map_err(erro)
    }

    /// O histórico de estado de todas as versões, do mais antigo ao mais novo.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn rastro(&self, regra: &str) -> Result<Vec<LinhaDoRastro>, String> {
        let c = self.catalogo()?;
        let mut linhas = Vec::new();
        for v in c.versoes(regra).map_err(erro)? {
            for (de, para, quem, motivo, em) in
                c.historico_de_estado(regra, v.versao).map_err(erro)?
            {
                linhas.push(LinhaDoRastro {
                    versao: v.versao,
                    de,
                    para,
                    quem,
                    motivo,
                    em,
                });
            }
        }
        linhas.sort_by(|a, b| (&a.em, a.versao).cmp(&(&b.em, b.versao)));
        Ok(linhas)
    }

    // ---- editor ----

    /// Valida o script no editor: `Ok(true)` verificado, `Ok(false)` sem executor para
    /// verificar.
    ///
    /// # Errors
    /// O texto do erro de sintaxe ou de "sem `transformar`".
    pub fn verificar_script(&self, fonte: &str) -> Result<bool, String> {
        match self.executor_ref() {
            Some(ex) => ex.verificar(fonte).map(|()| true).map_err(erro),
            None => Ok(false),
        }
    }

    /// Salva o que está no editor como uma versão-rascunho nova (autor: o usuário). Salvar o
    /// mesmo conteúdo da última versão devolve essa versão, sem criar outra.
    ///
    /// # Errors
    /// O texto do erro de definição, de script ou do catálogo.
    pub fn salvar_rascunho(
        &self,
        regra: &str,
        nome: &str,
        definicao_json: &str,
        script: &str,
        motivo: &str,
    ) -> Result<VersaoDeRegra, String> {
        let definicao = validar_definicao(definicao_json)?;
        let script = (!script.trim().is_empty()).then(|| script.to_owned());
        if let Some(fonte) = &script {
            self.verificar_script(fonte)?;
        }
        let motivo = if motivo.trim().is_empty() {
            "editada na janela".to_owned()
        } else {
            motivo.trim().to_owned()
        };
        self.catalogo()?
            .salvar_rascunho(NovaVersao {
                regra: regra.to_owned(),
                nome: nome.to_owned(),
                definicao,
                script,
                autor: Autor::Usuario,
                motivo,
            })
            .map_err(erro)
    }

    // ---- exemplos ----

    /// Roda os exemplos da versão.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn testar(&self, regra: &str, versao: u32) -> Result<RelatorioDosExemplos, String> {
        self.catalogo()?
            .testar(regra, versao, self.executor_ref())
            .map_err(erro)
    }

    /// Adiciona um exemplo a partir de texto colado. `esperado_json` é um [`Esperado`] em
    /// JSON.
    ///
    /// # Errors
    /// O texto do erro de nome, de JSON ou do catálogo.
    pub fn adicionar_exemplo(
        &self,
        regra: &str,
        nome: &str,
        texto: &str,
        esperado_json: &str,
    ) -> Result<(), String> {
        let nome = nome.trim();
        if nome.is_empty() {
            return Err("dê um nome ao exemplo".to_owned());
        }
        if texto.trim().is_empty() {
            return Err("cole o texto do documento".to_owned());
        }
        let esperado: Esperado = serde_json::from_str(esperado_json)
            .map_err(|e| format!("o resultado esperado não é um JSON válido: {e}"))?;
        self.catalogo()?
            .salvar_exemplo(regra, nome, &entrada_de_texto(texto), esperado)
            .map_err(erro)
    }

    /// O resultado esperado que a versão produz hoje para este texto (ponto de partida do
    /// exemplo): reconhece, com os campos lidos, ou "não se aplica".
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn esperado_de_hoje(
        &self,
        regra: &str,
        versao: u32,
        texto: &str,
    ) -> Result<Esperado, String> {
        let c = self.catalogo()?;
        let v = c.versao(regra, versao).map_err(erro)?;
        let motor = Motor::novo(vec![v], c.parametros().map_err(erro)?, None);
        let leitura = motor.ler_com(&entrada_de_texto(texto), self.executor_ref());
        Ok(match leitura.decisao {
            Decisao::Reconhecido(r) => Esperado::Reconhece {
                tipo_documental: r.tipo_documental,
                campos: r
                    .campos
                    .into_iter()
                    .map(|(n, c)| (n, c.valor))
                    .collect::<BTreeMap<String, Valor>>(),
            },
            _ => Esperado::NaoSeAplica,
        })
    }

    /// Apaga um exemplo.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn apagar_exemplo(&self, regra: &str, nome: &str) -> Result<(), String> {
        self.catalogo()?.apagar_exemplo(regra, nome).map_err(erro)
    }

    // ---- simulação ----

    /// O diff de trocar a versão em vigor da regra pela `versao`, lido sobre os textos dos
    /// exemplos e as entradas recentes. Só leitura: nada é gravado.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn simular(&self, regra: &str, versao: u32) -> Result<SimulacaoDaRegra, String> {
        let c = self.catalogo()?;
        let parametros = c.parametros().map_err(erro)?;
        let vigentes = c.vigentes().map_err(erro)?;
        let candidata = c.versao(regra, versao).map_err(erro)?;
        let mut com_candidata: Vec<VersaoDeRegra> = vigentes
            .iter()
            .filter(|v| v.regra != regra)
            .cloned()
            .collect();
        com_candidata.push(candidata);
        let atual = Motor::novo(vigentes, parametros.clone(), self.executor.clone());
        let candidato = Motor::novo(com_candidata, parametros, self.executor.clone());

        let mut rotulos = BTreeMap::new();
        let mut entradas = Vec::new();
        for ex in c.exemplos(regra).map_err(erro)? {
            rotulos.insert(ex.entrada.id.clone(), format!("exemplo «{}»", ex.nome));
            entradas.push(ex.entrada);
        }
        drop(c);
        if let Some(recentes) = &self.recentes {
            for e in recentes() {
                if !rotulos.contains_key(&e.id) {
                    rotulos.insert(e.id.clone(), "entrada recente".to_owned());
                    entradas.push(e);
                }
            }
        }
        Ok(SimulacaoDaRegra {
            versao,
            total: entradas.len(),
            diferencas: simular(&atual, &candidato, &entradas),
            rotulos,
        })
    }

    // ---- estado (só o usuário; D14) ----

    /// Por que a versão **não** pode ser ativada agora: um texto por exemplo que falha.
    /// Vazio quando nada bloqueia pelos exemplos.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn bloqueios_da_ativacao(&self, regra: &str, versao: u32) -> Result<Vec<String>, String> {
        Ok(self
            .testar(regra, versao)?
            .resultados
            .into_iter()
            .filter(|r| !r.passou)
            .map(|r| format!("{}: {}", r.nome, r.detalhe))
            .collect())
    }

    /// Ativa a versão. O catálogo bloqueia se um exemplo falha (D12).
    ///
    /// # Errors
    /// O texto do bloqueio ou do erro do catálogo.
    pub fn ativar(&self, regra: &str, versao: u32) -> Result<(), String> {
        self.catalogo()?
            .ativar(regra, versao, &Quem::Usuario, self.executor_ref())
            .map_err(erro)
    }

    /// Desativa a versão em vigor.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn desativar(&self, regra: &str) -> Result<(), String> {
        self.catalogo()?
            .desativar(regra, &Quem::Usuario)
            .map_err(erro)
    }

    /// Rollback: volta à versão que estava em vigor antes da atual.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn reverter(&self, regra: &str) -> Result<(), String> {
        self.catalogo()?
            .reverter(regra, &Quem::Usuario)
            .map_err(erro)
    }

    /// Consolida a versão, se o catálogo a sugere.
    ///
    /// # Errors
    /// O texto do erro do catálogo.
    pub fn consolidar(&self, regra: &str, versao: u32) -> Result<(), String> {
        self.catalogo()?
            .consolidar(regra, versao, &Quem::Usuario)
            .map_err(erro)
    }

    /// A regra tem sugestão de consolidar? Devolve a versão sugerida.
    pub fn consolidacao_sugerida(&self, regra: &str) -> Option<u32> {
        // O catálogo devolve (regra, aceitas seguidas); a versão sugerida é a ativa.
        self.sugestoes_de_consolidacao()
            .ok()?
            .into_iter()
            .find(|(r, _)| r == regra)?;
        self.versoes(regra)
            .ok()?
            .into_iter()
            .find(|v| v.estado == Estado::Ativa)
            .map(|v| v.versao)
    }

    // ---- demonstração ----

    /// Um catálogo em memória com três regras inventadas (nada real):
    ///
    /// - `comprovante-pix-banco-exemplo`: versão 2 **ativa** (a 1 foi revertida por uma
    ///   melhora), com exemplos que passam e cinco propostas aceitas seguidas, então a janela
    ///   sugere consolidar;
    /// - `fatura-cartao-banco-exemplo`: **rascunho** com um exemplo que falha (ativar fica
    ///   bloqueado);
    /// - `conta-de-energia-distribuidora-exemplo`: **consolidada**.
    ///
    /// # Errors
    /// O texto do erro do catálogo (não acontece em memória).
    pub fn ficticia(executor: Option<Arc<dyn ExecutorDeScript>>) -> Result<Self, String> {
        let mut c = Catalogo::em_memoria().map_err(erro)?;
        povoar_demonstracao(&mut c, executor.clone()).map_err(erro)?;
        Ok(Self::nova(Arc::new(Mutex::new(c)), executor))
    }
}

// ---------------------------------------------------------------------------------------
// Dados fictícios
// ---------------------------------------------------------------------------------------

const PIX_TEXTO: &str = "Banco Exemplo\nComprovante de Pix\nValor: R$ 1.250,00\nData: 12/10/2026\nPara: Maria Exemplo\nAutenticação: ABC123DEF456";
const PIX_TEXTO_2: &str = "Banco Exemplo\nComprovante de Pix\nValor: R$ 89,90\nData: 14/10/2026\nPara: Mercado Exemplo\nAutenticação: XYZ987";
const OUTRO_BANCO: &str = "Banco Imaginário\nComprovante de Pix\nValor: R$ 40,00\nData: 02/10/2026";
const FATURA_TEXTO: &str =
    "Cartão Exemplo\nFatura fechada\nTotal da fatura: R$ 2.310,45\nVencimento: 10/11/2026";
const ENERGIA_TEXTO: &str = "Distribuidora Exemplo de Energia\nConta de energia elétrica\nTotal a pagar: R$ 187,32\nVencimento: 20/10/2026";

fn extracao(regex: &str, formato: Formato) -> Extracao {
    Extracao {
        regex: regex.to_owned(),
        grupo: 1,
        formato,
        fonte: FonteDaExtracao::Texto,
    }
}

fn definicao_do_pix(com_favorecido: bool) -> Definicao {
    let mut d = Definicao::simples("comprovante-pix", "comprovante de pix");
    d.detectar
        .todos
        .push(Condicao::Contem("banco exemplo".to_owned()));
    d.extrair = BTreeMap::from([
        (
            "valor_centavos".to_owned(),
            extracao(r"(?i)valor:\s*R\$\s*([\d.,]+)", Formato::Centavos),
        ),
        (
            "data".to_owned(),
            extracao(r"(?i)data:\s*(\d{2}/\d{2}/\d{4})", Formato::Data),
        ),
    ]);
    if com_favorecido {
        d.extrair.insert(
            "favorecido".to_owned(),
            extracao(r"(?im)^para:\s*(.+)$", Formato::Texto),
        );
    }
    d.obrigatorios = vec!["valor_centavos".to_owned(), "data".to_owned()];
    d.propor = vec![ModeloDeProposta::ProvarRegistro { janela_dias: 2 }];
    d
}

fn definicao_da_fatura() -> Definicao {
    let mut d = Definicao::simples("fatura-de-cartao", "fatura fechada");
    d.detectar
        .todos
        .push(Condicao::Contem("cartao exemplo".to_owned()));
    d.extrair = BTreeMap::from([
        (
            "valor_centavos".to_owned(),
            extracao(r"(?i)total da fatura:\s*R\$\s*([\d.,]+)", Formato::Centavos),
        ),
        (
            "vencimento".to_owned(),
            extracao(r"(?i)vencimento:\s*(\d{2}/\d{2}/\d{4})", Formato::Data),
        ),
    ]);
    d.obrigatorios = vec!["valor_centavos".to_owned(), "vencimento".to_owned()];
    d.propor = vec![ModeloDeProposta::PagarFatura];
    d
}

fn definicao_da_energia() -> Definicao {
    let mut d = Definicao::simples("conta-de-energia", "conta de energia eletrica");
    d.detectar
        .todos
        .push(Condicao::Contem("distribuidora exemplo".to_owned()));
    d.extrair = BTreeMap::from([
        (
            "valor_centavos".to_owned(),
            extracao(r"(?i)total a pagar:\s*R\$\s*([\d.,]+)", Formato::Centavos),
        ),
        (
            "data".to_owned(),
            extracao(r"(?i)vencimento:\s*(\d{2}/\d{2}/\d{4})", Formato::Data),
        ),
    ]);
    d.obrigatorios = vec!["valor_centavos".to_owned(), "data".to_owned()];
    d.propor = vec![ModeloDeProposta::ConciliarOcorrencia {
        janela_dias: 3,
        compromisso: Some("Energia".to_owned()),
    }];
    d
}

fn nova(regra: &str, nome: &str, definicao: Definicao, autor: Autor, motivo: &str) -> NovaVersao {
    NovaVersao {
        regra: regra.to_owned(),
        nome: nome.to_owned(),
        definicao,
        script: None,
        autor,
        motivo: motivo.to_owned(),
    }
}

fn reconhece(tipo: &str, campos: &[(&str, Valor)]) -> Esperado {
    Esperado::Reconhece {
        tipo_documental: tipo.to_owned(),
        campos: campos
            .iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    }
}

/// Cinco propostas aceitas seguidas: lê entradas distintas e o usuário aceita cada uma.
fn aceitar_cinco(
    c: &mut Catalogo,
    executor: Option<Arc<dyn ExecutorDeScript>>,
    modelo: impl Fn(u32) -> String,
) -> Result<(), ErroDasRegras> {
    for n in 0..5 {
        let motor = Motor::das_vigentes(c, executor.clone())?;
        let entrada = entrada_de_texto(&modelo(n));
        let leitura = motor.ler(&entrada);
        c.registrar_execucao(&leitura)?;
        if let Decisao::Reconhecido(r) = &leitura.decisao {
            c.registrar_proposta(r, &entrada.id)?;
            c.registrar_retorno(&r.chave, Retorno::Aceito, &Quem::Usuario, "aceita")?;
        }
    }
    Ok(())
}

fn povoar_demonstracao(
    c: &mut Catalogo,
    executor: Option<Arc<dyn ExecutorDeScript>>,
) -> Result<(), ErroDasRegras> {
    const PIX: &str = "comprovante-pix-banco-exemplo";
    const NOME_PIX: &str = "Comprovante Pix · Banco Exemplo";
    // Pix: a versão 1 (sem favorecido) entrou em vigor; a 2, que lê o favorecido, a substituiu.
    let v1 = c.salvar_rascunho(nova(
        PIX,
        NOME_PIX,
        definicao_do_pix(false),
        Autor::Usuario,
        "primeira versão, a partir de um comprovante",
    ))?;
    let esperado_1 = reconhece(
        "comprovante-pix",
        &[
            ("valor_centavos", Valor::Centavos(125_000)),
            ("data", Valor::Data("2026-10-12".to_owned())),
        ],
    );
    c.salvar_exemplo(
        PIX,
        "Pix de R$ 1.250,00",
        &entrada_de_texto(PIX_TEXTO),
        esperado_1,
    )?;
    c.salvar_exemplo(
        PIX,
        "Pix de outro banco",
        &entrada_de_texto(OUTRO_BANCO),
        Esperado::NaoSeAplica,
    )?;
    c.ativar(PIX, v1.versao, &Quem::Usuario, executor.as_deref())?;
    let v2 = c.salvar_rascunho(nova(
        PIX,
        NOME_PIX,
        definicao_do_pix(true),
        Autor::Usuario,
        "passa a ler o favorecido",
    ))?;
    c.salvar_exemplo(
        PIX,
        "Pix para o mercado",
        &entrada_de_texto(PIX_TEXTO_2),
        reconhece(
            "comprovante-pix",
            &[
                ("valor_centavos", Valor::Centavos(8_990)),
                ("favorecido", Valor::Texto("Mercado Exemplo".to_owned())),
            ],
        ),
    )?;
    c.ativar(PIX, v2.versao, &Quem::Usuario, executor.as_deref())?;

    // A energia: ativa, cinco aceitas seguidas e consolidada pelo usuário.
    const ENERGIA: &str = "conta-de-energia-distribuidora-exemplo";
    let e1 = c.salvar_rascunho(nova(
        ENERGIA,
        "Conta de energia · Distribuidora Exemplo",
        definicao_da_energia(),
        Autor::Fabrica,
        "regra de fábrica fictícia",
    ))?;
    c.salvar_exemplo(
        ENERGIA,
        "Conta de outubro",
        &entrada_de_texto(ENERGIA_TEXTO),
        reconhece(
            "conta-de-energia",
            &[("valor_centavos", Valor::Centavos(18_732))],
        ),
    )?;
    c.ativar(ENERGIA, e1.versao, &Quem::Usuario, executor.as_deref())?;
    aceitar_cinco(c, executor.clone(), |n| {
        format!(
            "Distribuidora Exemplo de Energia\nConta de energia elétrica\nTotal a pagar: R$ {},{:02}\nVencimento: {:02}/10/2026",
            150 + n,
            10 + n,
            10 + n
        )
    })?;
    c.consolidar(ENERGIA, e1.versao, &Quem::Usuario)?;

    // O Pix ganha cinco aceitas seguidas depois da consolidação da energia (a sugestão).
    aceitar_cinco(c, executor, |n| {
        format!(
            "Banco Exemplo\nComprovante de Pix\nValor: R$ {},00\nData: {:02}/10/2026\nPara: Pessoa {n} Exemplo",
            20 + n * 15,
            3 + n
        )
    })?;

    // A fatura: rascunho cujo exemplo espera o valor errado.
    const FATURA: &str = "fatura-cartao-banco-exemplo";
    c.salvar_rascunho(nova(
        FATURA,
        "Fatura do cartão · Banco Exemplo",
        definicao_da_fatura(),
        Autor::Ia,
        "proposta pela IA a partir de uma fatura",
    ))?;
    c.salvar_exemplo(
        FATURA,
        "Fatura de outubro",
        &entrada_de_texto(FATURA_TEXTO),
        reconhece(
            "fatura-de-cartao",
            &[("valor_centavos", Valor::Centavos(231_000))],
        ),
    )?;
    c.salvar_exemplo(
        FATURA,
        "Pix não é fatura",
        &entrada_de_texto(PIX_TEXTO),
        Esperado::NaoSeAplica,
    )?;
    Ok(())
}

/// O texto do estado, para a lista e os selos.
pub fn rotulo_do_estado(e: Estado) -> &'static str {
    match e {
        Estado::Rascunho => "rascunho",
        Estado::Ativa => "ativa",
        Estado::Consolidada => "consolidada",
        Estado::Desativada => "desativada",
    }
}

/// O texto do autor.
pub fn rotulo_do_autor(a: Autor) -> &'static str {
    match a {
        Autor::Usuario => "você",
        Autor::Ia => "IA (MCP)",
        Autor::Fabrica => "fábrica",
        Autor::Correcao => "correção",
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_regras::script::ExecutorFicticio;

    fn fonte() -> FonteDasRegras {
        FonteDasRegras::ficticia(None).expect("catálogo fictício")
    }

    const PIX: &str = "comprovante-pix-banco-exemplo";
    const FATURA: &str = "fatura-cartao-banco-exemplo";
    const ENERGIA: &str = "conta-de-energia-distribuidora-exemplo";

    fn resumo(f: &FonteDasRegras, id: &str) -> ResumoDaRegra {
        f.resumos()
            .expect("lista")
            .into_iter()
            .find(|r| r.id == id)
            .expect("regra")
    }

    #[test]
    fn a_demonstracao_tem_as_tres_regras_nos_tres_estados() {
        let f = fonte();
        assert_eq!(resumo(&f, PIX).estado, Some(Estado::Ativa));
        assert_eq!(resumo(&f, PIX).versao_em_vigor, Some(2));
        assert_eq!(resumo(&f, FATURA).estado, None);
        assert_eq!(resumo(&f, ENERGIA).estado, Some(Estado::Consolidada));
    }

    #[test]
    fn ativacao_bloqueada_quando_um_exemplo_falha() {
        let f = fonte();
        let bloqueios = f.bloqueios_da_ativacao(FATURA, 1).expect("bloqueios");
        assert_eq!(bloqueios.len(), 1, "{bloqueios:?}");
        assert!(bloqueios[0].starts_with("Fatura de outubro"));
        let e = f.ativar(FATURA, 1).expect_err("deve bloquear");
        assert!(e.contains("Fatura de outubro"), "{e}");
        assert_eq!(resumo(&f, FATURA).estado, None);
    }

    #[test]
    fn corrigido_o_exemplo_a_ativacao_passa() {
        let f = fonte();
        let bom = r#"{"esperado":"reconhece","tipo_documental":"fatura-de-cartao","campos":{"valor_centavos":{"tipo":"centavos","valor":231045}}}"#;
        f.adicionar_exemplo(FATURA, "Fatura de outubro", FATURA_TEXTO, bom)
            .expect("substitui o exemplo");
        assert!(f.bloqueios_da_ativacao(FATURA, 1).expect("b").is_empty());
        f.ativar(FATURA, 1).expect("ativa");
        assert_eq!(resumo(&f, FATURA).estado, Some(Estado::Ativa));
    }

    #[test]
    fn salvar_rascunho_cria_versao_nova_do_usuario() {
        let f = fonte();
        let atual = f.versoes(PIX).expect("v").pop().expect("última");
        let mut d = atual.definicao.clone();
        d.prioridade = 7;
        let nova = f
            .salvar_rascunho(
                PIX,
                "Comprovante Pix · Banco Exemplo",
                &definicao_em_texto(&d),
                "",
                "sobe a prioridade",
            )
            .expect("salva");
        assert_eq!(nova.versao, atual.versao + 1);
        assert_eq!(nova.estado, Estado::Rascunho);
        assert_eq!(nova.autor, Autor::Usuario);
        assert_eq!(nova.motivo, "sobe a prioridade");
        // A versão em vigor não mudou.
        assert_eq!(resumo(&f, PIX).versao_em_vigor, Some(atual.versao));
        // O mesmo conteúdo de novo não cria outra.
        let igual = f
            .salvar_rascunho(
                PIX,
                "Comprovante Pix · Banco Exemplo",
                &definicao_em_texto(&d),
                "",
                "x",
            )
            .expect("salva");
        assert_eq!(igual.versao, nova.versao);
    }

    #[test]
    fn json_invalido_e_regex_ruim_nao_salvam() {
        let f = fonte();
        assert!(f.salvar_rascunho(PIX, "n", "{", "", "").is_err());
        let mut d = definicao_do_pix(true);
        d.extrair.insert("x".into(), extracao("(", Formato::Texto));
        let e = f
            .salvar_rascunho(PIX, "n", &definicao_em_texto(&d), "", "")
            .expect_err("regex ruim");
        assert!(!e.is_empty());
    }

    #[test]
    fn script_so_salva_se_o_executor_verifica() {
        let f = fonte();
        let d = definicao_em_texto(&definicao_do_pix(true));
        // Sem executor, o script não é verificado, mas o rascunho salva (a ativação é que
        // bloqueia no catálogo).
        assert_eq!(f.verificar_script("function transformar() end"), Ok(false));
        let ex = ExecutorFicticio::novo();
        let com: FonteDasRegras = FonteDasRegras::nova(f.catalogo.clone(), Some(Arc::new(ex)));
        assert!(com.verificar_script("function transformar() end").is_err());
        assert!(com.salvar_rascunho(PIX, "n", &d, "lixo", "").is_err());
    }

    #[test]
    fn consolidar_so_aparece_quando_sugerido() {
        let f = fonte();
        // O Pix acertou cinco seguidas: sugerido. A energia já está consolidada e a fatura
        // nem está em vigor: não.
        assert_eq!(f.consolidacao_sugerida(PIX), Some(2));
        assert_eq!(f.consolidacao_sugerida(ENERGIA), None);
        assert_eq!(f.consolidacao_sugerida(FATURA), None);
        assert!(f.consolidar(FATURA, 1).is_err());
        f.consolidar(PIX, 2).expect("consolida");
        assert_eq!(resumo(&f, PIX).estado, Some(Estado::Consolidada));
        assert_eq!(f.consolidacao_sugerida(PIX), None);
    }

    #[test]
    fn reverter_troca_a_versao_em_vigor() {
        let f = fonte();
        assert_eq!(resumo(&f, PIX).versao_em_vigor, Some(2));
        f.reverter(PIX).expect("reverte");
        assert_eq!(resumo(&f, PIX).versao_em_vigor, Some(1));
        let rastro = f.rastro(PIX).expect("rastro");
        assert!(
            rastro
                .iter()
                .any(|l| l.versao == 1 && l.motivo.contains("reativada no rollback"))
        );
        // Reverter a regra sem versão anterior dá erro claro.
        assert!(f.reverter(ENERGIA).is_err());
    }

    #[test]
    fn desativar_tira_a_regra_de_vigor() {
        let f = fonte();
        f.desativar(PIX).expect("desativa");
        assert_eq!(resumo(&f, PIX).estado, None);
    }

    #[test]
    fn exemplos_adicionar_apagar_e_o_esperado_de_hoje() {
        let f = fonte();
        let antes = f.exemplos(PIX).expect("e").len();
        let esperado = f.esperado_de_hoje(PIX, 2, PIX_TEXTO).expect("esperado");
        let json = serde_json::to_string(&esperado).expect("json");
        f.adicionar_exemplo(PIX, "Outro", PIX_TEXTO, &json)
            .expect("add");
        assert_eq!(f.exemplos(PIX).expect("e").len(), antes + 1);
        assert!(f.testar(PIX, 2).expect("t").aprovado());
        f.apagar_exemplo(PIX, "Outro").expect("apaga");
        assert_eq!(f.exemplos(PIX).expect("e").len(), antes);
        assert!(f.adicionar_exemplo(PIX, " ", "x", "{}").is_err());
        assert!(f.adicionar_exemplo(PIX, "n", "x", "não é json").is_err());
    }

    #[test]
    fn simular_mostra_antes_e_depois_sem_texto() {
        let f = fonte();
        let v1 = f.simular(PIX, 1).expect("simula");
        // A versão 1 não lê o favorecido: o exemplo do mercado muda.
        assert!(!v1.diferencas.mudaram.is_empty());
        let m = &v1.diferencas.mudaram[0];
        assert!(v1.rotulos[&m.entrada].starts_with("exemplo"));
        let texto = format!("{:?}", v1.diferencas);
        assert!(!texto.contains("Mercado Exemplo\nAut"));
        // Simular a versão em vigor não muda nada.
        let v2 = f.simular(PIX, 2).expect("simula");
        assert!(v2.diferencas.mudaram.is_empty());
        assert_eq!(v2.diferencas.iguais, v2.total);
    }

    #[test]
    fn a_ia_nao_ativa_pelo_catalogo() {
        // A costura sempre pede como usuário; o catálogo recusa os outros (D14).
        let f = fonte();
        let r = f
            .catalogo
            .lock()
            .expect("lock")
            .ativar(FATURA, 1, &Quem::Ia, None);
        assert!(matches!(r, Err(ErroDasRegras::NaoPermitido(_))));
    }
}
