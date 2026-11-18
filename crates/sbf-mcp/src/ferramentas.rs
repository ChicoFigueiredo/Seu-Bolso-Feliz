//! As ferramentas do servidor MCP, como funções sobre o banco. Cada uma devolve um
//! `serde_json::Value` enxuto e paginado, ou uma mensagem de erro para o modelo.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use sbf_inventario::banco::abrir_leitura;
use sbf_inventario::migracoes::VERSAO_ATUAL;
use sbf_inventario::sugestao::{CAMPOS_DO_ACHADO, CAMPOS_DO_REGISTRO};
use sbf_triagem::entrada::carregar_item;
use sbf_triagem::normalizar::{cortar, truncar_texto};
use sbf_triagem::prompt::LIMITE_DA_JUSTIFICATIVA;
use sbf_triagem::resposta::validar_sugestao;
use sbf_triagem::sugestoes::{
    EstadoDaSugestao, NovaSugestao, atualizar_rodada, gravar, iniciar_rodada,
};
use sbf_triagem::{Alvo, fila};
use serde::Deserialize;
use serde_json::{Value, json};

/// Itens por página, se o modelo não disser.
pub const LIMITE_PADRAO: i64 = 25;
/// O máximo de itens por página.
pub const LIMITE_MAXIMO: i64 = 100;
/// O máximo de sugestões por chamada de `sugerir`.
pub const MAXIMO_DE_SUGESTOES: usize = 100;
/// O `modelo` das sugestões que chegam por aqui.
pub const MODELO_MCP: &str = "mcp";
/// A versão do "prompt" das sugestões do MCP (não há prompt nosso; marca a origem).
pub const VERSAO_MCP: &str = "mcp-1";

type Resultado = Result<Value, String>;

fn erro<E: std::fmt::Display>(e: E) -> String {
    format!("falha ao consultar: {e}")
}

/// O que o servidor enxerga: o `inventario.sqlite` dado e, se existir, o cache do livro.
#[derive(Debug, Clone)]
pub struct Nucleo {
    banco: PathBuf,
    livro: Option<PathBuf>,
    semelhantes: Semelhantes,
    plano: Option<crate::plano::Plano>,
    regras: Option<crate::regras::Regras>,
}

/// A busca por semelhança do índice local e o nível de confiança deste cliente MCP.
#[derive(Clone)]
struct Semelhantes(Option<(Arc<dyn sbf_indice::Busca>, sbf_ia::NivelDeConfianca)>);

impl std::fmt::Debug for Semelhantes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Semelhantes")
            .field("ligada", &self.0.is_some())
            .finish()
    }
}

/// O máximo de trechos por chamada de `buscar_semelhantes`.
pub const MAXIMO_DE_SEMELHANTES: usize = 20;
/// O `destinatario` de `saida_ia` para esta ferramenta.
pub const DESTINATARIO_MCP: &str = "mcp:buscar_semelhantes";

/// A paginação comum: `limite` (1 a 100, padrão 25) e `deslocamento` (quantos pular).
#[derive(Debug, Clone, Copy, Default)]
struct Pagina {
    limite: i64,
    deslocamento: i64,
}

impl Pagina {
    fn de(limite: Option<i64>, deslocamento: Option<i64>) -> Self {
        Self {
            limite: limite.unwrap_or(LIMITE_PADRAO).clamp(1, LIMITE_MAXIMO),
            deslocamento: deslocamento.unwrap_or(0).max(0),
        }
    }

    /// O envelope da resposta; `proximo_deslocamento` some na última página.
    fn envelope(self, total: i64, itens: Vec<Value>) -> Value {
        let proximo = self.deslocamento + i64::try_from(itens.len()).unwrap_or(0);
        let mut v = json!({ "total": total, "itens": itens });
        if proximo < total {
            v["proximo_deslocamento"] = json!(proximo);
        }
        v
    }
}

/// Escapa `%`, `_` e `\` para um `LIKE ... ESCAPE '\'`.
fn padrao_like(t: &str) -> String {
    let mut s = String::from("%");
    for c in t.trim().chars() {
        if matches!(c, '%' | '_' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('%');
    s
}

/// Um `WHERE` montado aos poucos, com os parâmetros na ordem.
#[derive(Default)]
struct Onde {
    condicoes: Vec<String>,
    valores: Vec<Sql>,
}

impl Onde {
    /// Todo `?` da condição vira o mesmo parâmetro numerado.
    fn e(&mut self, condicao: &str, valor: impl Into<Sql>) {
        self.condicoes
            .push(condicao.replace('?', &format!("?{}", self.valores.len() + 1)));
        self.valores.push(valor.into());
    }

    fn sem_valor(&mut self, condicao: &str) {
        self.condicoes.push(condicao.to_owned());
    }

    fn texto(&self) -> String {
        if self.condicoes.is_empty() {
            "1".to_owned()
        } else {
            self.condicoes.join(" AND ")
        }
    }
}

fn limpo(t: &Option<String>) -> Option<&str> {
    t.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn curto(t: Option<String>, max: usize) -> Value {
    t.map_or(Value::Null, |t| {
        Value::String(cortar(&t.replace('\n', " "), max))
    })
}

// ---------------------------------------------------------------------------------------
// Parâmetros (o `servidor` os usa também como esquema)

#[derive(Debug, Default, Deserialize)]
pub struct ParamsDeRegistros {
    pub texto: Option<String>,
    pub fornecedor: Option<String>,
    pub categoria: Option<String>,
    pub de: Option<String>,
    pub ate: Option<String>,
    pub estado: Option<String>,
    pub sem_categoria: Option<bool>,
    pub limite: Option<i64>,
    pub deslocamento: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ParamsDeAchados {
    pub estado: Option<String>,
    pub tipo: Option<String>,
    pub texto: Option<String>,
    pub limite: Option<i64>,
    pub deslocamento: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ParamsDeFornecedores {
    pub texto: Option<String>,
    pub limite: Option<i64>,
    pub deslocamento: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ParamsDeDecisoes {
    pub alvo_tipo: Option<String>,
    pub limite: Option<i64>,
    pub deslocamento: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ParamsDoLivro {
    pub texto: Option<String>,
    pub categoria: Option<String>,
    pub conta: Option<String>,
    pub de: Option<String>,
    pub ate: Option<String>,
    pub sem_categoria: Option<bool>,
    pub limite: Option<i64>,
    pub deslocamento: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ParamsDeSugestoesAbertas {
    pub alvo_tipo: Option<String>,
    pub campo: Option<String>,
    pub modelo: Option<String>,
    pub confianca_minima: Option<f64>,
    pub limite: Option<i64>,
    pub deslocamento: Option<i64>,
}

/// Uma sugestão que o modelo propõe.
#[derive(Debug, Clone, Deserialize)]
pub struct SugestaoProposta {
    pub alvo_tipo: String,
    pub alvo_id: String,
    pub campo: String,
    pub valor: Value,
    pub confianca: f64,
    pub justificativa: String,
}

impl Nucleo {
    /// Prepara o núcleo sobre `banco` (o `inventario.sqlite`, que tem de existir: nada é
    /// criado) e o cache do livro-caixa, se houver.
    ///
    /// # Errors
    /// O arquivo não existe ou não abre como inventário.
    pub fn novo(banco: &Path, livro: Option<&Path>) -> Result<Self, String> {
        if !banco.is_file() {
            return Err(format!("não achei {} (nada foi criado)", banco.display()));
        }
        abrir_leitura(banco)
            .and_then(|c| c.query_row("SELECT COUNT(*) FROM sqlite_master", [], |_| Ok(())))
            .map_err(|e| format!("não deu para abrir o inventário: {e}"))?;
        Ok(Self {
            banco: banco.to_path_buf(),
            livro: livro.filter(|l| l.is_file()).map(Path::to_path_buf),
            semelhantes: Semelhantes(None),
            plano: None,
            regras: None,
        })
    }

    /// Liga `buscar_semelhantes` ao índice local. `nivel` é o nível de confiança deste
    /// cliente MCP (um destinatário novo nasce mascarado).
    #[must_use]
    pub fn com_busca(
        mut self,
        busca: Arc<dyn sbf_indice::Busca>,
        nivel: sbf_ia::NivelDeConfianca,
    ) -> Self {
        self.semelhantes = Semelhantes(Some((busca, nivel)));
        self
    }

    /// Liga as ferramentas do controle mensal. `fabrica` abre uma instância do serviço de
    /// planejamento por chamada. Chame depois de `com_busca`, para herdar o nível do cliente.
    #[must_use]
    pub fn com_planejamento(mut self, fabrica: Arc<crate::plano::Fabrica>) -> Self {
        self.plano = Some(crate::plano::Plano {
            fabrica,
            banco: self.banco.clone(),
            nivel: self.nivel(),
            hoje: None,
        });
        self
    }

    /// Liga as ferramentas de regras de reconhecimento sobre `caminho` (o `regras.sqlite`
    /// do cofre). `executor` roda os scripts (o processo filho do próprio `sbf`, ver
    /// [`crate::regras::executor_do_processo`]). Chame depois de `com_busca`, para herdar o
    /// nível do cliente.
    #[must_use]
    pub fn com_regras(
        mut self,
        caminho: PathBuf,
        executor: Arc<dyn sbf_regras::script::ExecutorDeScript>,
    ) -> Self {
        self.regras = Some(crate::regras::Regras::novo(
            caminho,
            self.banco.clone(),
            executor,
            self.nivel(),
        ));
        self
    }

    /// Há regras de reconhecimento ligadas?
    #[must_use]
    pub fn tem_regras(&self) -> bool {
        self.regras.is_some()
    }

    /// As ferramentas de regras.
    ///
    /// # Errors
    /// Regras não ligadas.
    pub fn regras(&self) -> Result<&crate::regras::Regras, String> {
        self.regras.as_ref().ok_or_else(|| {
            "as regras de reconhecimento não estão ligadas (falta <cofre>/regras.sqlite)".to_owned()
        })
    }

    /// Fixa o "hoje" do planejamento (para testes).
    #[must_use]
    pub fn com_hoje(mut self, hoje: sbf_planejamento::Data) -> Self {
        if let Some(p) = &mut self.plano {
            p.hoje = Some(hoje);
        }
        self
    }

    /// Há planejamento ligado?
    #[must_use]
    pub fn tem_planejamento(&self) -> bool {
        self.plano.is_some()
    }

    /// As ferramentas do controle mensal.
    ///
    /// # Errors
    /// Planejamento não ligado.
    pub fn plano(&self) -> Result<&crate::plano::Plano, String> {
        self.plano.as_ref().ok_or_else(|| {
            "o planejamento não está ligado (falta <cofre>/planejamento.sqlite)".to_owned()
        })
    }

    // ---------------------------------------------------------------- buscar_semelhantes

    /// Os trechos do índice local mais parecidos com `texto` (achados, registros e
    /// movimentações), mascarados no nível do cliente. O que saiu fica em `saida_ia`. Para
    /// ver o item inteiro, use `ver_registro`.
    ///
    /// # Errors
    /// Índice não ligado, texto vazio, falha da busca ou do registro da saída.
    pub fn buscar_semelhantes(&self, texto: &str, limite: Option<i64>) -> Resultado {
        let Some((busca, nivel)) = &self.semelhantes.0 else {
            return Err(
                "o índice local não está ligado (abra o app e deixe o worker indexar)".to_owned(),
            );
        };
        let texto = texto.trim();
        if texto.is_empty() {
            return Err("diga o texto a buscar".to_owned());
        }
        let k = usize::try_from(limite.unwrap_or(5))
            .unwrap_or(5)
            .clamp(1, MAXIMO_DE_SEMELHANTES);
        let achados = busca.parecidos(texto, k, None).map_err(erro)?;
        let itens: Vec<Value> = achados
            .iter()
            .map(|p| {
                json!({
                    "tipo": p.item.tipo,
                    "id": p.item.id,
                    "nota": (f64::from(p.nota) * 1000.0).round() / 1000.0,
                    "trecho": sbf_indice::perguntar::mascarar(&p.texto, *nivel),
                })
            })
            .collect();
        let resposta = json!({ "itens": itens });
        let bytes = resposta.to_string().len() as u64;
        if !itens.is_empty() {
            use sbf_ia::Registro as _;
            let registro =
                sbf_inventario::sugestao::RegistroDaIa::abrir(&self.banco, None).map_err(erro)?;
            registro
                .registrar_saida(&sbf_ia::Saida {
                    destinatario: DESTINATARIO_MCP.to_owned(),
                    provedor: "mcp".to_owned(),
                    modelo: "-".to_owned(),
                    nivel: *nivel,
                    bytes_enviados: bytes,
                    consumo: sbf_ia::Consumo::default(),
                    quando: sbf_ia::agora_rfc3339(),
                })
                .map_err(|e| format!("não deu para registrar a saída: {e}"))?;
        }
        Ok(resposta)
    }

    fn leitura(&self) -> Result<Connection, String> {
        abrir_leitura(&self.banco).map_err(erro)
    }

    // ---------------------------------------------------------------- inventario_resumo

    /// Os totais do inventário e do que a triagem já sabe.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn inventario_resumo(&self) -> Resultado {
        let c = self.leitura()?;
        let n =
            |sql: &str| -> Result<i64, String> { c.query_row(sql, [], |l| l.get(0)).map_err(erro) };
        let por = |sql: &str| -> Result<Value, String> {
            let mut st = c.prepare(sql).map_err(erro)?;
            let mut m = serde_json::Map::new();
            for l in st
                .query_map([], |l| {
                    Ok((l.get::<_, Option<String>>(0)?, l.get::<_, i64>(1)?))
                })
                .map_err(erro)?
            {
                let (k, v) = l.map_err(erro)?;
                m.insert(k.unwrap_or_else(|| "(vazio)".to_owned()), json!(v));
            }
            Ok(Value::Object(m))
        };
        let periodo: (Option<String>, Option<String>) = c
            .query_row(
                "SELECT MIN(substr(data, 1, 10)), MAX(substr(data, 1, 10)) FROM registro",
                [],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .map_err(erro)?;
        let versao: i64 = c
            .query_row("PRAGMA user_version", [], |l| l.get(0))
            .map_err(erro)?;
        let triagem = versao >= i64::from(VERSAO_ATUAL);
        let abertas = if triagem {
            por("SELECT campo, COUNT(*) FROM sugestao_ia
                  WHERE estado = 'aberta' AND campo <> 'nao_classificado' GROUP BY campo")?
        } else {
            json!({})
        };
        Ok(json!({
            "achados": {
                "total": n("SELECT COUNT(*) FROM achado")?,
                "por_estado": por("SELECT estado, COUNT(*) FROM achado GROUP BY estado")?,
                "decididos_pelo_usuario": n("SELECT COUNT(*) FROM achado WHERE decidido_por = 'usuario'")?,
            },
            "registros": {
                "total": n("SELECT COUNT(*) FROM registro")?,
                "por_estado": por("SELECT estado, COUNT(*) FROM registro GROUP BY estado")?,
                "sem_categoria": n("SELECT COUNT(*) FROM registro WHERE categoria_provavel IS NULL OR categoria_provavel = ''")?,
                "decididos_pelo_usuario": n("SELECT COUNT(*) FROM registro WHERE decidido_por = 'usuario'")?,
                "promovidos": n("SELECT COUNT(*) FROM registro WHERE estado = 'promovido' OR movimentacao_id IS NOT NULL")?,
                "primeira_data": periodo.0,
                "ultima_data": periodo.1,
            },
            "fornecedores": n("SELECT COUNT(DISTINCT COALESCE(fornecedor_normalizado, supplier_name_raw)) FROM registro")?,
            "sugestoes_abertas_por_campo": abertas,
            "sugerir_disponivel": triagem,
            "livro_caixa_disponivel": self.livro.is_some(),
        }))
    }

    // ---------------------------------------------------------------- registros

    /// Busca registros com filtros, mais novos primeiro.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn buscar_registros(&self, p: &ParamsDeRegistros) -> Resultado {
        let c = self.leitura()?;
        let mut o = Onde::default();
        if let Some(t) = limpo(&p.texto) {
            let like = padrao_like(t);
            o.e(
                "(descricao_original LIKE ? ESCAPE '\\' OR supplier_name_raw LIKE ? ESCAPE '\\' OR fornecedor_normalizado LIKE ? ESCAPE '\\')",
                like,
            );
        }
        if let Some(f) = limpo(&p.fornecedor) {
            o.e(
                "(fornecedor_normalizado LIKE ? ESCAPE '\\' OR supplier_name_raw LIKE ? ESCAPE '\\')",
                padrao_like(f),
            );
        }
        if let Some(cat) = limpo(&p.categoria) {
            o.e("categoria_provavel = ? COLLATE NOCASE", cat.to_owned());
        }
        if p.sem_categoria == Some(true) {
            o.sem_valor("(categoria_provavel IS NULL OR categoria_provavel = '')");
        }
        if let Some(de) = limpo(&p.de) {
            o.e("substr(data, 1, 10) >= ?", de.to_owned());
        }
        if let Some(ate) = limpo(&p.ate) {
            o.e("substr(data, 1, 10) <= ?", ate.to_owned());
        }
        if let Some(e) = limpo(&p.estado) {
            o.e("estado = ?", e.to_owned());
        }
        let onde = o.texto();
        let total: i64 = c
            .query_row(
                &format!("SELECT COUNT(*) FROM registro WHERE {onde}"),
                params_from_iter(o.valores.iter()),
                |l| l.get(0),
            )
            .map_err(erro)?;
        let pg = Pagina::de(p.limite, p.deslocamento);
        let mut st = c
            .prepare(&format!(
                "SELECT id, substr(data, 1, 10), tipo, valor_centavos, descricao_original,
                        COALESCE(fornecedor_normalizado, supplier_name_raw), categoria_provavel,
                        conta_provavel, estado, decidido_por, content_hash IS NOT NULL
                   FROM registro WHERE {onde} ORDER BY data DESC, id DESC LIMIT {} OFFSET {}",
                pg.limite, pg.deslocamento
            ))
            .map_err(erro)?;
        let itens = st
            .query_map(params_from_iter(o.valores.iter()), |l| {
                Ok(json!({
                    "id": l.get::<_, i64>(0)?,
                    "data": l.get::<_, String>(1)?,
                    "tipo": l.get::<_, String>(2)?,
                    "valor_centavos": l.get::<_, i64>(3)?,
                    "descricao": curto(l.get(4)?, 120),
                    "fornecedor": l.get::<_, Option<String>>(5)?,
                    "categoria": l.get::<_, Option<String>>(6)?,
                    "conta": l.get::<_, Option<String>>(7)?,
                    "estado": l.get::<_, String>(8)?,
                    "decidido_por": l.get::<_, Option<String>>(9)?,
                    "tem_documento": l.get::<_, bool>(10)?,
                }))
            })
            .map_err(erro)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(erro)?;
        Ok(pg.envelope(total, itens))
    }

    /// Um registro inteiro, com sub-registros, documento de origem, sugestões abertas e
    /// decisões.
    ///
    /// # Errors
    /// Registro inexistente ou falha do SQLite.
    pub fn ver_registro(&self, id: i64) -> Resultado {
        let c = self.leitura()?;
        let mut v: Value = c
            .query_row(
                "SELECT tipo, substr(data, 1, 10), data_tipo, ciclo_id, valor_centavos,
                        descricao_original, supplier_name_raw, fornecedor_normalizado,
                        categoria_provavel, conta_provavel, confianca, extraido_por,
                        conferido_por, estado, decidido_por, movimentacao_id, fonte_id,
                        content_hash
                   FROM registro WHERE id = ?1",
                [id],
                |l| {
                    Ok(json!({
                        "id": id,
                        "tipo": l.get::<_, String>(0)?,
                        "data": l.get::<_, String>(1)?,
                        "data_tipo": l.get::<_, String>(2)?,
                        "ciclo": l.get::<_, Option<String>>(3)?,
                        "valor_centavos": l.get::<_, i64>(4)?,
                        "descricao": curto(l.get(5)?, 500),
                        "fornecedor_bruto": l.get::<_, Option<String>>(6)?,
                        "fornecedor": l.get::<_, Option<String>>(7)?,
                        "categoria": l.get::<_, Option<String>>(8)?,
                        "conta": l.get::<_, Option<String>>(9)?,
                        "confianca": l.get::<_, Option<f64>>(10)?,
                        "extraido_por": l.get::<_, String>(11)?,
                        "conferido_por": l.get::<_, Option<String>>(12)?,
                        "estado": l.get::<_, String>(13)?,
                        "decidido_por": l.get::<_, Option<String>>(14)?,
                        "promovido": l.get::<_, Option<String>>(15)?.is_some() || l.get::<_, String>(13)? == "promovido",
                        "fonte": l.get::<_, String>(16)?,
                        "achado": l.get::<_, Option<String>>(17)?,
                    }))
                },
            )
            .optional()
            .map_err(erro)?
            .ok_or_else(|| format!("não existe o registro {id}"))?;
        let mut st = c
            .prepare(
                "SELECT tipo, descricao_original, valor_centavos, parcela_numero, parcela_total
                   FROM sub_registro WHERE registro_id = ?1 ORDER BY id LIMIT 30",
            )
            .map_err(erro)?;
        v["sub_registros"] = Value::Array(
            st.query_map([id], |l| {
                Ok(json!({
                    "tipo": l.get::<_, String>(0)?,
                    "descricao": curto(l.get(1)?, 120),
                    "valor_centavos": l.get::<_, Option<i64>>(2)?,
                    "parcela": l.get::<_, Option<i64>>(3)?,
                    "de": l.get::<_, Option<i64>>(4)?,
                }))
            })
            .map_err(erro)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(erro)?,
        );
        v["sugestoes_abertas"] = self.sugestoes_do_alvo(&c, "registro", &id.to_string())?;
        v["decisoes"] = decisoes_do_alvo(&c, "registro", &id.to_string())?;
        Ok(v)
    }

    fn sugestoes_do_alvo(&self, c: &Connection, tipo: &str, id: &str) -> Result<Value, String> {
        if !tem_triagem(c) {
            return Ok(json!([]));
        }
        let mut st = c
            .prepare(
                "SELECT id, campo, valor_json, confianca, justificativa, modelo FROM sugestao_ia
                  WHERE alvo_tipo = ?1 AND alvo_id = ?2 AND estado = 'aberta'
                    AND campo <> 'nao_classificado' ORDER BY id DESC LIMIT 20",
            )
            .map_err(erro)?;
        Ok(Value::Array(
            st.query_map(params![tipo, id], |l| linha_de_sugestao(l, 0))
                .map_err(erro)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(erro)?,
        ))
    }

    // ---------------------------------------------------------------- achados

    /// Busca achados (conteúdos únicos) por estado, tipo de documento e texto.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn buscar_achados(&self, p: &ParamsDeAchados) -> Resultado {
        let c = self.leitura()?;
        let mut o = Onde::default();
        if let Some(e) = limpo(&p.estado) {
            o.e("a.estado = ?", e.to_owned());
        }
        if let Some(t) = limpo(&p.tipo) {
            o.e("a.document_type = ? COLLATE NOCASE", t.to_owned());
        }
        if let Some(t) = limpo(&p.texto) {
            o.e(
                "(a.supplier_name_raw LIKE ? ESCAPE '\\' OR EXISTS (
                    SELECT 1 FROM origem o WHERE o.content_hash = a.content_hash
                      AND (o.filename LIKE ? ESCAPE '\\' OR o.gmail_subject LIKE ? ESCAPE '\\')))",
                padrao_like(t),
            );
        }
        let onde = o.texto();
        let total: i64 = c
            .query_row(
                &format!("SELECT COUNT(*) FROM achado a WHERE {onde}"),
                params_from_iter(o.valores.iter()),
                |l| l.get(0),
            )
            .map_err(erro)?;
        let pg = Pagina::de(p.limite, p.deslocamento);
        let mut st = c
            .prepare(&format!(
                "SELECT a.content_hash, a.estado, a.document_type, a.supplier_name_raw, a.data_chave,
                        a.valor_centavos, a.decidido_por, a.mime_type,
                        EXISTS (SELECT 1 FROM texto t WHERE t.content_hash = a.content_hash),
                        (SELECT COALESCE(NULLIF(o.gmail_subject, ''), o.filename) FROM origem o
                          WHERE o.content_hash = a.content_hash ORDER BY o.id LIMIT 1)
                   FROM achado a WHERE {onde} ORDER BY a.primeiro_visto_em DESC, a.content_hash
                  LIMIT {} OFFSET {}",
                pg.limite, pg.deslocamento
            ))
            .map_err(erro)?;
        let itens = st
            .query_map(params_from_iter(o.valores.iter()), |l| {
                Ok(json!({
                    "content_hash": l.get::<_, String>(0)?,
                    "estado": l.get::<_, String>(1)?,
                    "tipo": l.get::<_, Option<String>>(2)?,
                    "fornecedor": l.get::<_, Option<String>>(3)?,
                    "data_chave": l.get::<_, Option<String>>(4)?,
                    "valor_centavos": l.get::<_, Option<i64>>(5)?,
                    "decidido_por": l.get::<_, Option<String>>(6)?,
                    "mime": l.get::<_, Option<String>>(7)?,
                    "tem_texto": l.get::<_, bool>(8)?,
                    "nome": curto(l.get(9)?, 100),
                }))
            })
            .map_err(erro)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(erro)?;
        Ok(pg.envelope(total, itens))
    }

    /// O nível de confiança deste cliente MCP: o da busca por semelhança, se ligada; senão o
    /// padrão, mascarado (um destinatário novo nasce mascarado).
    pub(crate) fn nivel(&self) -> sbf_ia::NivelDeConfianca {
        self.semelhantes
            .0
            .as_ref()
            .map_or(sbf_ia::NivelDeConfianca::Mascarado, |(_, n)| *n)
    }

    /// O texto extraído de um achado, truncado como na triagem: 3000 caracteres do começo
    /// e 1000 do fim, com os números longos mascarados no nível de confiança do cliente.
    ///
    /// # Errors
    /// Achado inexistente ou sem texto.
    pub fn ver_texto_do_achado(&self, content_hash: &str) -> Resultado {
        let c = self.leitura()?;
        let achado: Option<(String, Option<String>)> = c
            .query_row(
                "SELECT a.estado, t.conteudo FROM achado a
                   LEFT JOIN texto t ON t.content_hash = a.content_hash WHERE a.content_hash = ?1",
                [content_hash.trim()],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .optional()
            .map_err(erro)?;
        let Some((estado, texto)) = achado else {
            return Err("não existe achado com esse content_hash".to_owned());
        };
        let Some(texto) = texto else {
            return Err(
                "esse achado não tem texto extraído (o documento pode ser só imagem)".to_owned(),
            );
        };
        let enviado = sbf_indice::perguntar::mascarar(&truncar_texto(&texto), self.nivel());
        Ok(json!({
            "content_hash": content_hash.trim(),
            "estado": estado,
            "caracteres_no_original": texto.chars().count(),
            "truncado": enviado.contains("caracteres omitidos"),
            "texto": enviado,
        }))
    }

    // ---------------------------------------------------------------- fornecedores

    /// Fornecedores dos registros, com a contagem e a categoria aprendida das decisões do
    /// usuário (só quando todas concordam).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn fornecedores(&self, p: &ParamsDeFornecedores) -> Resultado {
        let c = self.leitura()?;
        let mut o = Onde::default();
        o.sem_valor("COALESCE(NULLIF(fornecedor_normalizado, ''), NULLIF(supplier_name_raw, '')) IS NOT NULL");
        if let Some(t) = limpo(&p.texto) {
            o.e(
                "(fornecedor_normalizado LIKE ? ESCAPE '\\' OR supplier_name_raw LIKE ? ESCAPE '\\')",
                padrao_like(t),
            );
        }
        let onde = o.texto();
        let nome = "COALESCE(NULLIF(fornecedor_normalizado, ''), supplier_name_raw)";
        let total: i64 = c
            .query_row(
                &format!("SELECT COUNT(DISTINCT {nome}) FROM registro WHERE {onde}"),
                params_from_iter(o.valores.iter()),
                |l| l.get(0),
            )
            .map_err(erro)?;
        let pg = Pagina::de(p.limite, p.deslocamento);
        let mut st = c
            .prepare(&format!(
                "SELECT {nome}, COUNT(*), SUM(categoria_provavel IS NULL OR categoria_provavel = ''),
                        group_concat(DISTINCT tipo)
                   FROM registro WHERE {onde} GROUP BY {nome}
                  ORDER BY COUNT(*) DESC, {nome} LIMIT {} OFFSET {}",
                pg.limite, pg.deslocamento
            ))
            .map_err(erro)?;
        let aprendidas = fila::categorias_aprendidas(&c).map_err(erro)?;
        let itens = st
            .query_map(params_from_iter(o.valores.iter()), |l| {
                Ok((
                    l.get::<_, String>(0)?,
                    l.get::<_, i64>(1)?,
                    l.get::<_, i64>(2)?,
                    l.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(erro)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(erro)?
            .into_iter()
            .map(|(nome, n, sem, tipos)| {
                let chave = sbf_triagem::normalizar::normalizar_fornecedor(&nome);
                // Uma categoria por tipo; mostra as que o usuário já fixou.
                let aprendida: BTreeMap<String, Value> = tipos
                    .as_deref()
                    .unwrap_or("")
                    .split(',')
                    .filter_map(|t| {
                        aprendidas.get(&(chave.clone(), t.to_owned())).map(|a| {
                            (
                                t.to_owned(),
                                json!({"categoria": a.categoria, "decisoes": a.exemplos}),
                            )
                        })
                    })
                    .collect();
                json!({
                    "fornecedor": nome,
                    "registros": n,
                    "sem_categoria": sem,
                    "categoria_aprendida_por_tipo": aprendida,
                })
            })
            .collect();
        Ok(pg.envelope(total, itens))
    }

    // ---------------------------------------------------------------- decisões

    /// As decisões mais recentes do log (mais novas primeiro).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn decisoes_recentes(&self, p: &ParamsDeDecisoes) -> Resultado {
        let c = self.leitura()?;
        let mut o = Onde::default();
        if let Some(t) = limpo(&p.alvo_tipo) {
            o.e("alvo_tipo = ?", t.to_owned());
        }
        let onde = o.texto();
        let total: i64 = c
            .query_row(
                &format!("SELECT COUNT(*) FROM decisao WHERE {onde}"),
                params_from_iter(o.valores.iter()),
                |l| l.get(0),
            )
            .map_err(erro)?;
        let pg = Pagina::de(p.limite, p.deslocamento);
        let mut st = c
            .prepare(&format!(
                "SELECT id, alvo_tipo, alvo_id, campo, antes, depois, autor, motivo, em, desfaz
                   FROM decisao WHERE {onde} ORDER BY id DESC LIMIT {} OFFSET {}",
                pg.limite, pg.deslocamento
            ))
            .map_err(erro)?;
        let itens = st
            .query_map(params_from_iter(o.valores.iter()), linha_de_decisao)
            .map_err(erro)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(erro)?;
        Ok(pg.envelope(total, itens))
    }

    // ---------------------------------------------------------------- livro-caixa

    /// Movimentações do cache local do livro-caixa (só leitura). Sem cache, devolve erro.
    ///
    /// # Errors
    /// Sem cache do livro ou falha do SQLite.
    pub fn livro_movimentacoes(&self, p: &ParamsDoLivro) -> Resultado {
        let Some(caminho) = &self.livro else {
            return Err(
                "não há cache do livro-caixa nesta estação (sincronize o app primeiro)".to_owned(),
            );
        };
        let c = abrir_leitura(caminho).map_err(erro)?;
        let nomes = |colecao: &str| -> Result<BTreeMap<String, String>, String> {
            let mut st = c
                .prepare("SELECT id, dados FROM linha WHERE colecao = ?1")
                .map_err(erro)?;
            let mut m = BTreeMap::new();
            for l in st
                .query_map([colecao], |l| {
                    Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?))
                })
                .map_err(erro)?
            {
                let (id, dados) = l.map_err(erro)?;
                if let Some(n) = serde_json::from_str::<Value>(&dados)
                    .ok()
                    .and_then(|v| v.get("nome").and_then(Value::as_str).map(str::to_owned))
                {
                    m.insert(id, n);
                }
            }
            Ok(m)
        };
        let categorias = nomes("categorias")?;
        let contas = nomes("contas")?;
        let mut st = c
            .prepare("SELECT dados FROM linha WHERE colecao = 'movimentacoes' ORDER BY id")
            .map_err(erro)?;
        let texto = limpo(&p.texto).map(str::to_lowercase);
        let categoria = limpo(&p.categoria).map(str::to_lowercase);
        let conta = limpo(&p.conta).map(str::to_lowercase);
        let busca = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        let mut achadas: Vec<Value> = Vec::new();
        for linha in st.query_map([], |l| l.get::<_, String>(0)).map_err(erro)? {
            let Ok(v) = serde_json::from_str::<Value>(&linha.map_err(erro)?) else {
                continue;
            };
            let data = busca(&v, "data").unwrap_or_default();
            let nome_cat = busca(&v, "categoria").and_then(|id| categorias.get(&id).cloned());
            let nome_conta = busca(&v, "conta").and_then(|id| contas.get(&id).cloned());
            let descricao = busca(&v, "descricao");
            if let Some(de) = limpo(&p.de)
                && data.as_str() < de
            {
                continue;
            }
            if let Some(ate) = limpo(&p.ate)
                && data.as_str() > ate
            {
                continue;
            }
            if p.sem_categoria == Some(true) && nome_cat.is_some() {
                continue;
            }
            if let Some(t) = &texto
                && !descricao
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(t.as_str())
            {
                continue;
            }
            if let Some(k) = &categoria
                && nome_cat.as_deref().map(str::to_lowercase).as_deref() != Some(k.as_str())
            {
                continue;
            }
            if let Some(k) = &conta
                && nome_conta.as_deref().map(str::to_lowercase).as_deref() != Some(k.as_str())
            {
                continue;
            }
            achadas.push(json!({
                "id": busca(&v, "id"),
                "data": data,
                "tipo": v.get("tipo"),
                "valor_centavos": v.get("valor"),
                "descricao": curto(descricao, 120),
                "categoria": nome_cat,
                "conta": nome_conta,
                "confirmada": v.get("confirmada"),
            }));
        }
        achadas.sort_by(|a, b| b["data"].as_str().cmp(&a["data"].as_str()));
        let total = i64::try_from(achadas.len()).unwrap_or(i64::MAX);
        let pg = Pagina::de(p.limite, p.deslocamento);
        let itens = achadas
            .into_iter()
            .skip(usize::try_from(pg.deslocamento).unwrap_or(0))
            .take(usize::try_from(pg.limite).unwrap_or(0))
            .collect();
        Ok(pg.envelope(total, itens))
    }

    // ---------------------------------------------------------------- sugestões

    /// As sugestões abertas (as do MCP e as das outras rodadas), mais novas primeiro.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn sugestoes_abertas(&self, p: &ParamsDeSugestoesAbertas) -> Resultado {
        let c = self.leitura()?;
        if !tem_triagem(&c) {
            return Err("o inventário ainda não tem a triagem (abra o app e migre)".to_owned());
        }
        let mut o = Onde::default();
        o.sem_valor("estado = 'aberta' AND campo <> 'nao_classificado'");
        if let Some(t) = limpo(&p.alvo_tipo) {
            o.e("alvo_tipo = ?", t.to_owned());
        }
        if let Some(t) = limpo(&p.campo) {
            o.e("campo = ?", t.to_owned());
        }
        if let Some(t) = limpo(&p.modelo) {
            o.e("modelo = ?", t.to_owned());
        }
        if let Some(m) = p.confianca_minima {
            o.e("confianca >= ?", m);
        }
        let onde = o.texto();
        let total: i64 = c
            .query_row(
                &format!("SELECT COUNT(*) FROM sugestao_ia WHERE {onde}"),
                params_from_iter(o.valores.iter()),
                |l| l.get(0),
            )
            .map_err(erro)?;
        let pg = Pagina::de(p.limite, p.deslocamento);
        let mut st = c
            .prepare(&format!(
                "SELECT id, campo, valor_json, confianca, justificativa, modelo, alvo_tipo, alvo_id
                   FROM sugestao_ia WHERE {onde} ORDER BY id DESC LIMIT {} OFFSET {}",
                pg.limite, pg.deslocamento
            ))
            .map_err(erro)?;
        let itens = st
            .query_map(params_from_iter(o.valores.iter()), |l| {
                let mut v = linha_de_sugestao(l, 0)?;
                v["alvo_tipo"] = json!(l.get::<_, String>(6)?);
                v["alvo_id"] = json!(l.get::<_, String>(7)?);
                Ok(v)
            })
            .map_err(erro)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(erro)?;
        Ok(pg.envelope(total, itens))
    }

    /// Grava sugestões em `sugestao_ia` (modelo `mcp`), numa rodada própria. Cada uma é
    /// validada; as inválidas voltam com o motivo e as válidas entram. **Nunca grava
    /// decisão.**
    ///
    /// # Errors
    /// Lista vazia ou grande demais, banco sem a triagem, falha do SQLite.
    pub fn sugerir(&self, sugestoes: &[SugestaoProposta]) -> Resultado {
        if sugestoes.is_empty() {
            return Err("a lista de sugestões veio vazia".to_owned());
        }
        if sugestoes.len() > MAXIMO_DE_SUGESTOES {
            return Err(format!(
                "no máximo {MAXIMO_DE_SUGESTOES} sugestões por chamada"
            ));
        }
        let leitura = self.leitura()?;
        if !tem_triagem(&leitura) {
            return Err("o inventário ainda não tem a triagem (abra o app e migre)".to_owned());
        }
        let mut validas: Vec<(usize, Alvo, sbf_triagem::resposta::SugestaoValida)> = Vec::new();
        let mut recusadas: Vec<Value> = Vec::new();
        for (i, s) in sugestoes.iter().enumerate() {
            match validar_proposta(&leitura, s) {
                Ok((alvo, v)) => validas.push((i, alvo, v)),
                Err(motivo) => recusadas.push(json!({
                    "posicao": i, "alvo_tipo": s.alvo_tipo, "alvo_id": s.alvo_id,
                    "campo": s.campo, "motivo": motivo,
                })),
            }
        }
        drop(leitura);
        let mut gravadas = 0usize;
        let mut rodada = Value::Null;
        if !validas.is_empty() {
            let escrita = sbf_triagem::sugestoes::abrir(&self.banco).map_err(erro)?;
            let id = iniciar_rodada(&escrita, MODELO_MCP, false).map_err(erro)?;
            escrita.execute_batch("BEGIN IMMEDIATE").map_err(erro)?;
            let feito = (|| -> Result<(), String> {
                for (_, alvo, v) in &validas {
                    // Uma sugestão nova do MCP para o mesmo alvo e campo substitui a antiga.
                    escrita
                        .execute(
                            "UPDATE sugestao_ia SET estado = 'obsoleta'
                              WHERE alvo_tipo = ?1 AND alvo_id = ?2 AND campo = ?3
                                AND modelo = ?4 AND estado = 'aberta'",
                            params![alvo.tipo(), alvo.id(), v.campo, MODELO_MCP],
                        )
                        .map_err(erro)?;
                    gravar(
                        &escrita,
                        &NovaSugestao::da_ia(
                            alvo,
                            v,
                            MODELO_MCP,
                            VERSAO_MCP,
                            id,
                            EstadoDaSugestao::Aberta,
                        ),
                    )
                    .map_err(erro)?;
                    gravadas += 1;
                }
                atualizar_rodada(
                    &escrita,
                    id,
                    i64::try_from(gravadas).unwrap_or(0),
                    0,
                    0,
                    "concluida",
                    true,
                )
                .map_err(erro)?;
                Ok(())
            })();
            match feito {
                Ok(()) => escrita.execute_batch("COMMIT").map_err(erro)?,
                Err(e) => {
                    let _ = escrita.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }
            rodada = json!(id);
        }
        Ok(json!({
            "gravadas": gravadas,
            "recusadas": recusadas,
            "rodada": rodada,
            "aviso": "são sugestões: nada foi decidido. O usuário confirma em Inventário > Triagem por IA.",
        }))
    }
}

/// O banco tem `sugestao_ia` (migração 3 em diante)?
fn tem_triagem(c: &Connection) -> bool {
    c.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'sugestao_ia')",
        [],
        |l| l.get(0),
    )
    .unwrap_or(false)
}

/// Valida uma proposta: alvo existente, não decidido pelo usuário, não promovido, campo
/// permitido e valor aceito.
fn validar_proposta(
    c: &Connection,
    s: &SugestaoProposta,
) -> Result<(Alvo, sbf_triagem::resposta::SugestaoValida), String> {
    let alvo = Alvo::de(s.alvo_tipo.trim(), s.alvo_id.trim()).ok_or_else(|| {
        "alvo_tipo deve ser \"achado\" ou \"registro\" (registro com id numérico)".to_owned()
    })?;
    let campos: &[&str] = match alvo {
        Alvo::Achado(_) => &CAMPOS_DO_ACHADO,
        Alvo::Registro(_) => &CAMPOS_DO_REGISTRO,
    };
    if !campos.contains(&s.campo.as_str()) {
        return Err(format!(
            "campo \"{}\" não é permitido para {}; use um de: {}",
            s.campo,
            alvo.tipo(),
            campos.join(", ")
        ));
    }
    if s.justificativa.chars().count() > LIMITE_DA_JUSTIFICATIVA {
        return Err(format!(
            "justificativa passa de {LIMITE_DA_JUSTIFICATIVA} caracteres"
        ));
    }
    let (existe, decidido, promovido): (bool, bool, bool) = match &alvo {
        Alvo::Achado(h) => c
            .query_row(
                "SELECT COALESCE(decidido_por, '') = 'usuario', estado NOT IN ('novo', 'duvida')
                   FROM achado WHERE content_hash = ?1",
                [h],
                |l| Ok((true, l.get(0)?, l.get(1)?)),
            )
            .optional()
            .map_err(erro)?
            .unwrap_or((false, false, false)),
        Alvo::Registro(id) => c
            .query_row(
                "SELECT COALESCE(decidido_por, '') = 'usuario',
                        estado = 'promovido' OR movimentacao_id IS NOT NULL
                   FROM registro WHERE id = ?1",
                [id],
                |l| Ok((true, l.get(0)?, l.get(1)?)),
            )
            .optional()
            .map_err(erro)?
            .unwrap_or((false, false, false)),
    };
    if !existe {
        return Err("esse alvo não existe".to_owned());
    }
    if decidido {
        return Err("o usuário já decidiu esse item; a sugestão não vale".to_owned());
    }
    if promovido {
        return Err(match alvo {
            Alvo::Registro(_) => "o registro já foi promovido a movimentação",
            Alvo::Achado(_) => "o achado já foi resolvido (não está novo nem em dúvida)",
        }
        .to_owned());
    }
    let item = carregar_item(c, &alvo, true)
        .map_err(erro)?
        .ok_or_else(|| "esse alvo não existe".to_owned())?;
    let v = validar_sugestao(&item, &s.campo, &s.valor, s.confianca, Some(&s.justificativa))
        .ok_or_else(|| {
            "valor inválido para o campo (e_financeiro: sim|nao|incerto; tipo_documento: da lista; data_chave: AAAA-MM-DD; valor: centavos inteiros; tipo: da lista; duplicata_de: um dos registros parecidos mostrados em ver_registro) ou confiança fora de 0 a 1".to_owned()
        })?;
    Ok((alvo, v))
}

fn linha_de_sugestao(l: &rusqlite::Row<'_>, base: usize) -> rusqlite::Result<Value> {
    let valor: Option<String> = l.get(base + 2)?;
    Ok(json!({
        "id": l.get::<_, i64>(base)?,
        "campo": l.get::<_, String>(base + 1)?,
        "valor": valor.and_then(|v| serde_json::from_str::<Value>(&v).ok()),
        "confianca": l.get::<_, f64>(base + 3)?,
        "justificativa": l.get::<_, String>(base + 4)?,
        "modelo": l.get::<_, String>(base + 5)?,
    }))
}

fn linha_de_decisao(l: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let json_curto =
        |t: Option<String>| -> Value { t.map_or(Value::Null, |t| Value::String(cortar(&t, 200))) };
    Ok(json!({
        "id": l.get::<_, i64>(0)?,
        "alvo_tipo": l.get::<_, String>(1)?,
        "alvo_id": l.get::<_, String>(2)?,
        "campo": l.get::<_, String>(3)?,
        "antes": json_curto(l.get(4)?),
        "depois": json_curto(l.get(5)?),
        "autor": l.get::<_, String>(6)?,
        "motivo": curto(l.get(7)?, 120),
        "em": l.get::<_, String>(8)?,
        "desfaz": l.get::<_, Option<i64>>(9)?,
    }))
}

fn decisoes_do_alvo(c: &Connection, tipo: &str, id: &str) -> Result<Value, String> {
    let mut st = c
        .prepare(
            "SELECT id, alvo_tipo, alvo_id, campo, antes, depois, autor, motivo, em, desfaz
               FROM decisao WHERE alvo_tipo = ?1 AND alvo_id = ?2 ORDER BY id DESC LIMIT 10",
        )
        .map_err(erro)?;
    Ok(Value::Array(
        st.query_map(params![tipo, id], linha_de_decisao)
            .map_err(erro)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(erro)?,
    ))
}
