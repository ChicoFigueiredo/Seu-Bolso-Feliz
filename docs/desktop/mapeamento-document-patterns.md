# Mapeamento: `document_patterns` (web) × catálogo de regras (desktop)

Decisão D6 da spec 13: o formato do desktop é o **canônico**. Este documento põe os dois
lados lado a lado, campo a campo, para que quem mexer num deles saiba o que existe no outro.
**Não há sincronização nem remoção do TypeScript nesta fase.** O padrão web continua como
está; este mapeamento é só referência.

Fontes lidas:

- Web: `packages/db/src/schema.ts` (tabelas `document_patterns` e `pattern_feedback`) e
  `workers/ingestion/src/parsers/pattern-matcher.ts` (o que de fato é lido e aplicado).
- Desktop: `crates/sbf-regras/src/catalogo.rs` (migrações `ESQUEMA_1`, `salvar_rascunho`,
  `ativar`, `consolidar`, `registrar_retorno`) e `crates/sbf-regras/src/definicao.rs`.

Termos: "regra" e "versão" no desktop; "padrão" e "versão" na web (`CONTEXT.md` usa
**Regra de reconhecimento** para o conceito).

## 1. Tabela `document_patterns` (web) × desktop

| Campo web              | Tipo web                                          | Par no desktop                                                                                       | Situação                                                                                                |
| ---------------------- | ------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `id`                   | uuid                                              | `regra.id` (texto legível, ex.: `comprovante-pix-banco-exemplo`)                                     | Mudou de identidade: o desktop usa um id estável legível, não uuid.                                     |
| `user_id`              | uuid                                              | nenhum                                                                                               | Sem par. O cofre é de um dono só.                                                                       |
| `name`                 | texto                                             | `regra.nome` (e `versao_de_regra.definicao` não carrega o nome)                                      | Mapeia. Atualizado a cada rascunho com o mesmo id.                                                      |
| `supplier_id`          | uuid, fornecedor                                  | nenhum campo próprio; casa-se por condição `contem` ou `regex` sobre o texto                         | Sem par direto. O desktop não tem cadastro de fornecedor na regra.                                      |
| `document_type`        | texto                                             | `regra.tipo_documental` e `definicao.tipo_documental`                                                | Mapeia.                                                                                                 |
| `institution_id`       | uuid, instituição                                 | nenhum; a instituição aparece como condição de texto (`contem`)                                      | Sem par direto.                                                                                         |
| `extraction_rules`     | jsonb                                             | `definicao.detectar` (condições) e `definicao.extrair` (regex, grupo, formato, fonte)                | Parcial. O matcher web **não lê** este campo (só o seleciona), então não há formato fixo a converter.   |
| `field_mappings`       | jsonb (`campo_origem → destino`, com `transform`) | a chave de `extrair` já é o nome de destino; o `formato` cobre parte das transformações              | Parcial. Ver seção 3.                                                                                   |
| `sample_fingerprints`  | texto[]                                           | nenhum. O equivalente é a condição (`contem`, `regex`) e os **exemplos** (`exemplo_de_regra`)        | Sem par exato. Não é lido pelo matcher web.                                                             |
| `confidence_threshold` | numérico 0 a 1                                    | nenhum. O desktop não tem limiar: a decisão é por precedência, obrigatórios e revisão (`Ambiguo`)    | Sem par. Ver seção 2.                                                                                   |
| `version`              | inteiro                                           | `versao_de_regra.versao` (sequência por regra, a partir de 1)                                        | Mapeia.                                                                                                 |
| `is_active`            | booleano                                          | `versao_de_regra.estado` in (`ativa`, `consolidada`)                                                 | Mapeia com mudança de sentido: no desktop, uma versão salva **nasce rascunho** e só ativa pelo usuário. |
| `feedback_count`       | inteiro                                           | soma de `retorno` da regra (`ResumoDaRegra.retornos_total`, que conta aceito, corrigido e rejeitado) | Derivado. Não é gravado num contador.                                                                   |
| `success_count`        | inteiro                                           | `retorno` com `retorno = 'aceito'` (`ResumoDaRegra.retornos_aceitos`)                                | Derivado.                                                                                               |
| `created_at`           | timestamp                                         | `versao_de_regra.criada_em` (da versão) e `regra.criada_em` (da regra)                               | Mapeia.                                                                                                 |
| `updated_at`           | timestamp                                         | `historico_de_estado.em` (cada mudança de estado tem data)                                           | Sem coluna equivalente; o histórico é o rastro.                                                         |

Restrições da web sem par: `document_patterns_confidence_threshold_check` (0 a 1) e os
contadores não negativos. No desktop, as versões são imutáveis (trigger
`versao_de_regra_imutavel`) e não se apagam (`versao_de_regra_sem_apagar`). A web permite
alterar a linha; o desktop não.

## 2. Limiar de confiança (`confidence_threshold`)

A web aplica o padrão automaticamente quando `capped >= confidence_threshold`
(`autoApplied`). O desktop **não tem limiar**: o que decide é:

1. a precedência (`prioridade`, depois especificidade, `motor.rs`);
2. os campos obrigatórios (`obrigatorios`): sem eles, a regra não se aplica;
3. a ambiguidade: duas regras empatadas que discordam num campo crítico geram `Ambiguo`, e
   nada é gravado;
4. a conciliação, que só acontece sozinha com a regra **consolidada** e as três provas exatas.

Isso é uma troca deliberada: confiança numérica não prova nada (spec 13, §3.5), e a
conciliação automática depende de prova, não de um número.

## 3. Transformações (`field_mappings.transform`) × `formato`

O matcher web aplica `uppercase`, `lowercase`, `trim` e `parse_number`. O desktop tem o campo
`formato` em cada extração:

| Transformação web   | Par no desktop                                                 | Observação                                                                                                               |
| ------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `trim`              | `formato: texto` (o trecho da regex, sem espaço fora do grupo) | Par aproximado: a regex escolhe o trecho.                                                                                |
| `lowercase`         | `formato: minusculas`                                          | Par direto.                                                                                                              |
| `parse_number`      | `formato: centavos` ou `formato: inteiro`                      | Diferença: o web devolve um `float` em reais; o desktop devolve **centavos inteiros**, nunca ponto flutuante.            |
| `uppercase`         | nenhum                                                         | Sem par. Se for preciso, o caminho é um script (`script`) ou uma nova variante de `Formato`, decisão a tomar com o dono. |
| (sem transformação) | `formato: texto`, `data`, `competencia`                        | O desktop também tipa datas (`AAAA-MM-DD`) e competências (`AAAA-MM`).                                                   |

## 4. Tabela `pattern_feedback` (web) × `retorno` (desktop)

| Campo web                     | Tipo web  | Par no desktop                                                                                                                  | Situação                                                                               |
| ----------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `id`                          | uuid      | `retorno.id` (inteiro autoincremental)                                                                                          | Mapeia; muda o tipo da chave.                                                          |
| `user_id`                     | uuid      | `retorno.quem` (`usuario`, `ia`, `sistema`)                                                                                     | Parcial: o desktop guarda quem agiu, não o id da conta.                                |
| `pattern_id`                  | uuid, FK  | `retorno.regra` + `retorno.versao`                                                                                              | Mapeia (a versão é a unidade no desktop).                                              |
| `source_document_id`          | uuid, FK  | `retorno.chave` (chave de idempotência da proposta) → `proposta.entrada` (id da entrada: `sha256` do arquivo, ou `pluggy:<id>`) | Indireto. O `retorno` não guarda o documento; a proposta guarda.                       |
| `feedback_type` = `correct`   | texto     | `retorno = aceito`                                                                                                              | Mapeia.                                                                                |
| `feedback_type` = `incorrect` | texto     | `retorno = rejeitado`                                                                                                           | Mapeia.                                                                                |
| `feedback_type` = `partial`   | texto     | `retorno = corrigido` (aceito depois de mudar algum campo; zera a sequência para consolidar)                                    | Par aproximado.                                                                        |
| `feedback_type` = `improved`  | texto     | nenhum                                                                                                                          | Sem par. No desktop, melhorar é salvar uma **versão nova** (rascunho), não um retorno. |
| (sem equivalente web)         |           | `retorno = desfeito` (desfez uma conciliação automática; rebaixa a consolidada)                                                 | Só no desktop.                                                                         |
| `corrections`                 | jsonb     | nenhum no `retorno`. A correção vira uma **versão-rascunho** (`autor = correcao`, D7)                                           | Sem par no retorno.                                                                    |
| `notes`                       | texto     | `retorno.motivo`                                                                                                                | Mapeia.                                                                                |
| `created_at`                  | timestamp | `retorno.em`                                                                                                                    | Mapeia.                                                                                |

Restrição web `pattern_feedback_feedback_type_check` tem os quatro valores acima. O desktop
só aceita `aceito`, `corrigido`, `rejeitado` e `desfeito`.

## 5. Tabelas que existem só no desktop

Estas não têm par na web. Não são lacunas: são o que a spec 13 pede e o web não tinha.

| Tabela desktop         | Para que serve                                                                                                                                                                                                                   |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `regra`                | Id, nome, tipo documental, criação. Uma linha por regra.                                                                                                                                                                         |
| `versao_de_regra`      | Cada versão: `definicao` (JSON), `script`, `checksum` (sha256 do conteúdo), `autor` (`usuario`, `ia`, `fabrica`, `correcao`), `motivo`, `estado` (`rascunho`, `ativa`, `consolidada`, `desativada`), `aceitas_seguidas` e datas. |
| `historico_de_estado`  | Cada troca de estado, quem fez e por quê. Base do rollback (`reverter`).                                                                                                                                                         |
| `exemplo_de_regra`     | Os exemplos de teste (entrada e resultado esperado). Pré-requisito da ativação.                                                                                                                                                  |
| `historico_de_exemplo` | Os exemplos apagados ou trocados.                                                                                                                                                                                                |
| `execucao`             | Cada avaliação de uma regra numa entrada: resultado, motivo, duração e chave. Sem o texto do documento (AC-LUA-08).                                                                                                              |
| `parametro`            | Parâmetros versionados por competência (ex.: tabela do IRRF), imutáveis, com `fonte`.                                                                                                                                            |
| `proposta`             | As propostas geradas, idempotentes pela chave (AC-LUA-09), com estado `aberta`, `aceita`, `recusada` ou `desfeita`.                                                                                                              |

## 6. O que a web tem e o desktop não tem

- **Vínculo com fornecedor e instituição** (`supplier_id`, `institution_id`): o desktop
  trata isso como texto da entrada. Se o fornecedor virar dado próprio, a decisão é do dono.
- **Limiar numérico de confiança**: substituído pela regra de prova (seção 2).
- **Fingerprints de amostra**: substituídos por exemplos testáveis.
- **Contadores gravados** (`feedback_count`, `success_count`): derivados dos retornos.

## 7. Conclusão

O formato do desktop é o canônico. Um padrão web se traduz para ele assim: `name` → `nome`,
`document_type` → `tipo_documental`, `extraction_rules` e `field_mappings` → `definicao`,
`is_active` → estado da versão, e `pattern_feedback` → `retorno` (com `correct` → `aceito`,
`incorrect` → `rejeitado`, `partial` → `corrigido`). O que não tem par (limiar, fingerprint,
`improved`) não é convertido.

Não há sincronização nem remoção do TypeScript nesta fase: `document_patterns`,
`pattern_feedback` e o `pattern-matcher.ts` continuam como estão.
