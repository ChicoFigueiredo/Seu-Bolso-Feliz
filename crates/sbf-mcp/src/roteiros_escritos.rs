//! As ferramentas de roteiros escritos em Luau (spec 14, seção 7 e RE-19). **O MCP só propõe.**
//!
//! Ao contrário das regras de reconhecimento (ADR-021, D15), onde o MCP ativa, um roteiro
//! escrito comanda um navegador com credenciais. Por isso aqui há só três ferramentas:
//!
//! - `propor_roteiro_escrito`: valida o manifesto (as regras duras do `Manifesto`) e importa
//!   como **rascunho** com autor `mcp`. Nunca ativa, nunca executa, nunca testa.
//! - `listar_roteiros_escritos` e `ver_roteiro_escrito`: só leitura.
//!
//! Não existe ferramenta que ative, desative, execute ou teste. Ativar é só no terminal
//! interativo (`sbf roteiros escrito ativar`, com confirmação digitada) ou na janela; o tipo
//! `Confirmacao` do banco não se constrói sem terminal, e este módulo nem o importa.
//!
//! O catálogo é o `roteiros.sqlite` da **pasta de dados do app**, fora do cofre. Antes da
//! primeira escrita da sessão ele é copiado para `<cofre>/backups/mcp/` (ver
//! [`crate::backup`]), se já existir; se o backup falha, a escrita é recusada.
//!
//! Como nas outras ferramentas, cada chamada grava uma linha em `saida_ia` (destinatário
//! `mcp:<ferramenta>`), sem o conteúdo. A saída não passa pelo mascaramento de texto: o fonte
//! e o manifesto só guardam **rótulos** do chaveiro (nunca senha, CPF ou código), e mascarar
//! um script o corromperia.

use std::path::PathBuf;

use rmcp::schemars;
use sbf_ia::NivelDeConfianca;
use sbf_roteiros::ErroDeRoteiro;
use sbf_roteiros::banco::{AutorEscrito, BancoDeRoteiros, VersaoEscrita};
use sbf_roteiros::manifesto::{LIMITE_DE_BYTES, Manifesto};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::backup::{Backups, Qual};

/// Os nomes das ferramentas de roteiros escritos. São **todas**: nenhuma ativa, executa ou
/// testa. O servidor só as anuncia com [`RoteirosEscritos`] ligado.
pub const FERRAMENTAS: [&str; 3] = [
    "propor_roteiro_escrito",
    "listar_roteiros_escritos",
    "ver_roteiro_escrito",
];

/// O que o MCP **não** faz com roteiros escritos (a descrição do servidor e os testes citam).
pub const MENSAGEM_DE_PROPOSTA: &str = "rascunho criado; ativar só no terminal (`sbf roteiros escrito ativar`) ou na janela. O MCP nunca ativa nem executa roteiro escrito.";

const MAX_NOME: usize = 60;

type Resultado = Result<Value, String>;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeRoteiroEscrito {
    /// O manifesto, um objeto JSON: `nome` (kebab-case), `descricao`, `url_inicial` (https),
    /// `hosts` (só o host, sem esquema nem caminho; a allowlist), `rotulo_senha` e
    /// `rotulo_cpf` (rótulos do chaveiro, nunca o segredo), `mfa` (`{"tipo":"nenhum"}`,
    /// `{"tipo":"manual"}` ou `{"tipo":"email","remetente":"...","formato":"\\d{6}"}`, o e-mail
    /// só com `movimenta_dinheiro` false), `movimenta_dinheiro` (obrigatório), `classe`
    /// (`consulta` ou `atualizar_conexao`) e `agenda_horas`. Até 64 KiB.
    pub manifesto: Value,
    /// O fonte Luau do roteiro (só os verbos de `navegador` e `util`). Até 64 KiB.
    pub fonte: String,
    /// Por que esta versão existe, em até 500 caracteres.
    pub motivo: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeVerRoteiroEscrito {
    /// O nome do roteiro escrito (kebab-case).
    pub nome: String,
    /// O número da versão. Sem ele, a mais nova (veja `ativa` para saber a em vigor).
    pub versao: Option<u32>,
}

/// O que o servidor enxerga dos roteiros escritos: o `roteiros.sqlite`, o `inventario.sqlite`
/// (para `saida_ia`) e o nível de confiança deste cliente.
#[derive(Clone, Debug)]
pub struct RoteirosEscritos {
    caminho: PathBuf,
    banco: PathBuf,
    nivel: NivelDeConfianca,
    backups: Option<Backups>,
}

impl RoteirosEscritos {
    /// Os roteiros escritos sobre `caminho` (o `roteiros.sqlite`; a proposta o cria se ainda
    /// não existir, as leituras não).
    #[must_use]
    pub fn novo(caminho: PathBuf, banco: PathBuf, nivel: NivelDeConfianca) -> Self {
        Self {
            caminho,
            banco,
            nivel,
            backups: None,
        }
    }

    /// Liga o backup da primeira escrita da sessão.
    #[must_use]
    pub fn com_backups(mut self, backups: Backups) -> Self {
        self.backups = Some(backups);
        self
    }

    /// `propor_roteiro_escrito`: importa como **rascunho** com autor `mcp`. Não ativa, não
    /// executa, não testa. O mesmo conteúdo já importado como versão mais nova não duplica.
    ///
    /// # Errors
    /// Manifesto inválido (regras duras), fonte vazia ou maior que 64 KiB, motivo longo
    /// demais, backup que falhou ou falha do banco.
    pub fn propor(&self, p: &PedidoDeRoteiroEscrito) -> Resultado {
        self.executar("propor_roteiro_escrito", |s| s.proposta(p))
    }

    /// `listar_roteiros_escritos` (só leitura).
    ///
    /// # Errors
    /// Falha do banco.
    pub fn listar(&self) -> Resultado {
        self.executar("listar_roteiros_escritos", Self::listagem)
    }

    /// `ver_roteiro_escrito` (só leitura): manifesto, fonte, estado e histórico.
    ///
    /// # Errors
    /// Roteiro ou versão inexistente, ou falha do banco.
    pub fn ver(&self, p: &PedidoDeVerRoteiroEscrito) -> Resultado {
        self.executar("ver_roteiro_escrito", |s| s.visao(p))
    }

    fn executar(&self, ferramenta: &str, f: impl FnOnce(&Self) -> Resultado) -> Resultado {
        let r = f(self);
        let bytes = match &r {
            Ok(v) => v.to_string().len(),
            Err(e) => e.len(),
        };
        self.registrar(ferramenta, u64::try_from(bytes).unwrap_or(u64::MAX))?;
        r
    }

    fn registrar(&self, ferramenta: &str, bytes: u64) -> Result<(), String> {
        use sbf_ia::Registro as _;
        let registro = sbf_inventario::sugestao::RegistroDaIa::abrir(&self.banco, None)
            .map_err(|_| "não deu para registrar a saída".to_owned())?;
        registro
            .registrar_saida(&sbf_ia::Saida {
                destinatario: format!("mcp:{ferramenta}"),
                provedor: "mcp".to_owned(),
                modelo: "-".to_owned(),
                nivel: self.nivel,
                bytes_enviados: bytes,
                consumo: sbf_ia::Consumo::default(),
                quando: sbf_ia::agora_rfc3339(),
            })
            .map_err(|_| "não deu para registrar a saída".to_owned())
    }

    /// Antes da primeira escrita da sessão. Sem arquivo ainda, não há o que copiar.
    fn antes_de_escrever(&self) -> Result<(), String> {
        match &self.backups {
            Some(b) if self.caminho.is_file() => b.antes_de_escrever(Qual::Roteiros),
            _ => Ok(()),
        }
    }

    fn proposta(&self, p: &PedidoDeRoteiroEscrito) -> Resultado {
        // Primeiro tudo o que não escreve: um pedido inválido nem dispara backup.
        let json = serde_json::to_string(&p.manifesto)
            .map_err(|_| "o manifesto não é um JSON válido".to_owned())?;
        if json.len() > LIMITE_DE_BYTES {
            return Err("o manifesto passa de 64 KiB".to_owned());
        }
        if p.fonte.len() > LIMITE_DE_BYTES {
            return Err("o fonte do roteiro passa de 64 KiB".to_owned());
        }
        if p.fonte.trim().is_empty() {
            return Err("o fonte do roteiro está vazio".to_owned());
        }
        if p.motivo.chars().count() > 500 {
            return Err("o motivo passa de 500 caracteres".to_owned());
        }
        let manifesto = Manifesto::de_json(&json).map_err(pt)?;
        self.antes_de_escrever()?;
        let banco = BancoDeRoteiros::abrir(&self.caminho).map_err(pt)?;
        let importacao = banco
            .importar_escrito(&manifesto, &p.fonte, &AutorEscrito::Mcp, p.motivo.trim())
            .map_err(pt)?;
        let versao = banco
            .versao_escrita(&manifesto.nome, importacao.numero)
            .map_err(pt)?;
        let mensagem = if importacao.criada {
            MENSAGEM_DE_PROPOSTA.to_owned()
        } else {
            format!(
                "esta versão já existe (estado {}); nada foi criado. O MCP nunca ativa nem executa roteiro escrito.",
                versao.estado.texto()
            )
        };
        Ok(json!({
            "criado": importacao.criada,
            "nome": versao.roteiro,
            "versao": versao.numero,
            "estado": versao.estado.texto(),
            "autor": versao.autor.texto(),
            "checksum": versao.checksum,
            "mensagem": mensagem,
        }))
    }

    /// O banco só se já existe: ler não cria arquivo nenhum.
    fn banco_de_leitura(&self) -> Result<Option<BancoDeRoteiros>, String> {
        if !self.caminho.is_file() {
            return Ok(None);
        }
        BancoDeRoteiros::abrir(&self.caminho).map(Some).map_err(pt)
    }

    fn listagem(&self) -> Resultado {
        let Some(banco) = self.banco_de_leitura()? else {
            return Ok(
                json!({ "total": 0, "itens": [], "mensagem": "ainda não há roteiros escritos" }),
            );
        };
        let itens: Vec<Value> = banco
            .listar_escritos()
            .map_err(pt)?
            .into_iter()
            .map(|l| {
                json!({
                    "nome": l.nome,
                    "descricao": l.descricao,
                    "ativa": l.ativa,
                    "ultima": l.ultima,
                    "estado_da_ultima": l.estado_da_ultima.texto(),
                })
            })
            .collect();
        Ok(json!({ "total": itens.len(), "itens": itens }))
    }

    fn visao(&self, p: &PedidoDeVerRoteiroEscrito) -> Resultado {
        let nome = p.nome.trim();
        if nome.is_empty() || nome.chars().count() > MAX_NOME {
            return Err(format!("nome tem de ter de 1 a {MAX_NOME} caracteres"));
        }
        let banco = self
            .banco_de_leitura()?
            .ok_or_else(|| format!("roteiro escrito `{nome}` não encontrado"))?;
        let detalhe = banco.mostrar(nome).map_err(pt)?;
        let ativa = detalhe
            .versoes
            .iter()
            .find(|v| v.estado.texto() == "ativo")
            .map(|v| v.numero);
        let escolhida = match p.versao {
            Some(n) => detalhe
                .versoes
                .iter()
                .find(|v| v.numero == n)
                .ok_or_else(|| format!("roteiro escrito `{nome}` não tem a versão {n}"))?,
            None => detalhe
                .versoes
                .last()
                .ok_or_else(|| format!("roteiro escrito `{nome}` não tem versões"))?,
        };
        let versoes: Vec<Value> = detalhe
            .versoes
            .iter()
            .map(|v| {
                json!({
                    "versao": v.numero,
                    "estado": v.estado.texto(),
                    "autor": v.autor.texto(),
                    "criada_em": v.criada_em,
                })
            })
            .collect();
        let historico: Vec<Value> = detalhe
            .eventos
            .iter()
            .map(|(quando, versao, de, para, quem, detalhe)| {
                json!({
                    "quando": quando, "versao": versao, "de": de,
                    "para": para, "quem": quem, "detalhe": detalhe,
                })
            })
            .collect();
        Ok(json!({
            "nome": detalhe.nome,
            "ativa": ativa,
            "versao": conteudo(escolhida),
            "versoes": versoes,
            "historico": historico,
        }))
    }
}

fn conteudo(v: &VersaoEscrita) -> Value {
    json!({
        "numero": v.numero,
        "estado": v.estado.texto(),
        "autor": v.autor.texto(),
        "motivo": v.motivo,
        "criada_em": v.criada_em,
        "checksum": v.checksum,
        "manifesto": serde_json::to_value(&v.manifesto).unwrap_or(Value::Null),
        "fonte": v.fonte,
        "resultado_do_teste": v.resultado_do_teste,
    })
}

fn pt(e: ErroDeRoteiro) -> String {
    match e {
        ErroDeRoteiro::Banco(_) | ErroDeRoteiro::Disco(_) => {
            "falha ao consultar os roteiros escritos".to_owned()
        }
        outro => outro.to_string(),
    }
}
