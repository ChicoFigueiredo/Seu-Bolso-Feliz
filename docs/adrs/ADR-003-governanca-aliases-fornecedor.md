# ADR-003: Governança Técnica de Aliases de Fornecedor

**Status:** Aprovado
**Data:** 2026-03-21
**Origem:** Parecer Verônica — Ajuste Obrigatório 4.3
**Ata de referência:** `docs/refinos/2026-03/2026-03-21-13-30-checkpoint-pre-implementacao-ajustes-obrigatorios.md` (Seção 4)

---

## 1. Contexto

A tabela `supplier_aliases` permite que um fornecedor tenha múltiplos nomes (ex: "CELPE", "Neoenergia", "Companhia Energética de Pernambuco"). Porém, sem regras técnicas rígidas, o sistema corre risco de:

- Aliases conflitantes entre fornecedores (dois fornecedores com alias "CELPE")
- Fornecedores duplicados crescendo sem controle
- Merges inseguros que quebram histórico
- Associações retroativas incorretas

A Verônica exigiu que essas regras fossem formalizadas ANTES da implementação.

## 2. Decisão

### 2.1. Regra de Unicidade Temporal

> Para um dado `user_id`, NÃO podem existir dois aliases com o mesmo `alias_name` (case-insensitive) com períodos de vigência sobrepostos.

Isso significa: "CELPE" só pode apontar para UM fornecedor de cada vez. Se houve mudança de empresa (fusão), o alias antigo expira e um novo pode ser criado.

#### Trigger de Validação

```sql
CREATE OR REPLACE FUNCTION check_alias_uniqueness()
RETURNS TRIGGER AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM supplier_aliases sa
    WHERE sa.user_id = NEW.user_id
      AND sa.id != COALESCE(NEW.id, '00000000-0000-0000-0000-000000000000'::uuid)
      AND LOWER(sa.alias_name) = LOWER(NEW.alias_name)
      AND sa.is_active = true
      AND (
        -- Períodos se sobrepõem
        (sa.valid_from IS NULL OR NEW.valid_until IS NULL OR sa.valid_from <= NEW.valid_until)
        AND
        (sa.valid_until IS NULL OR NEW.valid_from IS NULL OR sa.valid_until >= NEW.valid_from)
      )
  ) THEN
    RAISE EXCEPTION 'Alias "%" já existe para outro fornecedor no período informado', NEW.alias_name;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_check_alias_uniqueness
  BEFORE INSERT OR UPDATE ON supplier_aliases
  FOR EACH ROW EXECUTE FUNCTION check_alias_uniqueness();
```

### 2.2. Regra de Vigência

| Campo         | Significado                   | Valor NULL                                    |
| ------------- | ----------------------------- | --------------------------------------------- |
| `valid_from`  | Desde quando o alias é válido | Sem data de início conhecida ("desde sempre") |
| `valid_until` | Até quando o alias é válido   | Ainda ativo (sem data de expiração)           |

**Combinações:**

- `valid_from = NULL, valid_until = NULL` → alias ativo em qualquer data
- `valid_from = 2020-01-01, valid_until = 2023-12-31` → alias válido de 2020 a 2023
- `valid_from = 2024-01-01, valid_until = NULL` → alias ativo desde 2024, sem fim definido

**Resolução temporal** (para buscar fornecedor por alias em data específica):

```sql
SELECT supplier_id FROM supplier_aliases
WHERE user_id = :user_id
  AND LOWER(alias_name) = LOWER(:alias)
  AND is_active = true
  AND (valid_from IS NULL OR valid_from <= :target_date)
  AND (valid_until IS NULL OR valid_until >= :target_date)
LIMIT 1;
```

### 2.3. Campo is_active

```sql
ALTER TABLE supplier_aliases ADD COLUMN is_active boolean DEFAULT true;
```

| Mecanismo                | Significado        | Quando usar                               |
| ------------------------ | ------------------ | ----------------------------------------- |
| `valid_until` preenchido | Expiração natural  | Empresa mudou de nome, fusão corporativa  |
| `is_active = false`      | Desativação manual | Alias errado, conflito detectado, limpeza |

> **REGRA:** Um alias com `is_active = false` NUNCA é usado na resolução, mesmo que esteja no período de vigência.

### 2.4. Auto-Alias em Renomeação

Quando o usuário renomeia um fornecedor (ex: "CELPE" → "Neoenergia"), o nome antigo é automaticamente preservado como alias:

```sql
CREATE OR REPLACE FUNCTION auto_alias_on_rename()
RETURNS TRIGGER AS $$
BEGIN
  IF OLD.name IS DISTINCT FROM NEW.name THEN
    INSERT INTO supplier_aliases (user_id, supplier_id, alias_name, alias_type, valid_until)
    VALUES (NEW.user_id, NEW.id, OLD.name, 'former_name', NOW());
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_auto_alias_on_rename
  BEFORE UPDATE ON suppliers
  FOR EACH ROW EXECUTE FUNCTION auto_alias_on_rename();
```

Isso garante que buscas pelo nome antigo continuem encontrando o fornecedor.

### 2.5. Fluxo de Merge de Fornecedores

Merge: fornecedor A é absorvido pelo fornecedor B.

#### Pré-condições

- Ambos pertencem ao mesmo `user_id`
- O usuário confirma explicitamente (confirmação humana obrigatória)
- O sistema exibe preview: "X transações, Y recorrências, Z documentos serão migrados"

#### Etapas (transação atômica via Edge Function)

```
Etapa 1 — Migrar referências de A → B:
  UPDATE transactions SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
  UPDATE recurring_templates SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
  UPDATE recurring_instances SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
  UPDATE statement_items SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
  UPDATE documents SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
  UPDATE liabilities SET supplier_id = B WHERE supplier_id = A AND user_id = :uid

Etapa 2 — Migrar aliases de A → B:
  UPDATE supplier_aliases SET supplier_id = B WHERE supplier_id = A AND user_id = :uid

Etapa 3 — Criar alias com nome do fornecedor absorvido:
  INSERT INTO supplier_aliases (supplier_id, alias_name, alias_type, valid_until)
  VALUES (B, A.name, 'former_name', NOW())

Etapa 4 — Migrar contratos de A → B:
  UPDATE supplier_contracts SET supplier_id = B WHERE supplier_id = A AND user_id = :uid

Etapa 5 — Migrar métricas de A → B:
  UPDATE consumption_metrics SET supplier_id = B WHERE supplier_id = A AND user_id = :uid

Etapa 6 — Migrar tags de A → B (sem duplicatas):
  INSERT INTO supplier_tags (supplier_id, tag_id)
  SELECT B, tag_id FROM supplier_tags WHERE supplier_id = A AND user_id = :uid
  ON CONFLICT DO NOTHING

Etapa 7 — Desativar fornecedor absorvido:
  UPDATE suppliers SET is_active = false,
    notes = CONCAT(notes, ' [Merged into ', B.name, ' at ', NOW(), ']')
  WHERE id = A

Etapa 8 — Registrar no audit_log:
  INSERT INTO audit_log (action, entity_type, entity_id, details)
  VALUES ('supplier_merge', 'supplier', B,
    '{"absorbed_id": "A.id", "absorbed_name": "A.name", "migrated_counts": {...}}'
  )
```

#### Pós-condições

- Todas as referências apontam para B
- A está inativo mas NÃO deletado (preserva rastro)
- Nome de A é alias de B com `type = 'former_name'`
- Audit log registra operação completa com contagem de migrações

#### Implementação

- **Edge Function** com `service_role_key` para atomicidade
- RLS é bypassado durante a operação (service role) e reativado após
- A Edge Function valida que ambos fornecedores pertencem ao mesmo user_id

### 2.6. Reversão de Merge

> **REGRA MVP:** Reversão automática de merge NÃO é implementada no MVP.

**Motivo:** Após o merge, o usuário pode ter editado transações, adicionado vinculações e alterado metadados. Reverter automaticamente é inseguro.

**Alternativa no MVP:**

1. Criar novo fornecedor com dados do antigo (manualmente)
2. Reatribuir transações desejadas ao novo fornecedor (manualmente ou em lote)
3. Consultar audit_log para recuperar dados do merge original

**Fase 2+:** Avaliar "undo merge" com janela de 24h, se nenhuma edição manual foi feita após o merge.

### 2.7. Revisão Humana Obrigatória

| Cenário                                               | Ação do sistema                       | Requer confirmação humana?              |
| ----------------------------------------------------- | ------------------------------------- | --------------------------------------- |
| Resolução de alias com match exato (case-insensitive) | Sugere fornecedor                     | **Não** no MVP — aceita automaticamente |
| Resolução de alias com match fuzzy (trigram)          | Sugere com % de confiança             | **Sim** — sempre                        |
| Merge de fornecedores                                 | Exibe preview com contagem de impacto | **Sim** — sempre                        |
| Associação retroativa em lote                         | Exibe lista de candidatos             | **Sim** — sempre (individual ou lote)   |
| Criação automática de alias (via renomeação)          | Auto-cria alias former_name           | **Não** — automático e seguro           |
| Detecção de possível duplicata                        | Sugere merge                          | **Sim** — sempre                        |
| Importação com alias sem match                        | Mostra "fornecedor desconhecido"      | **Sim** — usuário decide                |

### 2.8. Índice para Busca Fuzzy

```sql
-- Habilitar extensão trigram
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Índice para busca fuzzy em alias_name
CREATE INDEX idx_supplier_aliases_trgm
  ON supplier_aliases USING gin (alias_name gin_trgm_ops);

-- Índice para busca fuzzy em suppliers.name
CREATE INDEX idx_suppliers_name_trgm
  ON suppliers USING gin (name gin_trgm_ops);
```

**Busca combinada (nome + aliases):**

```sql
-- Autocomplete que busca em suppliers.name e supplier_aliases.alias_name
SELECT DISTINCT s.id, s.name,
  similarity(s.name, :query) AS name_score,
  (SELECT MAX(similarity(sa.alias_name, :query))
   FROM supplier_aliases sa
   WHERE sa.supplier_id = s.id AND sa.is_active = true
  ) AS alias_score
FROM suppliers s
WHERE s.user_id = :user_id AND s.is_active = true
  AND (
    s.name % :query
    OR EXISTS (
      SELECT 1 FROM supplier_aliases sa
      WHERE sa.supplier_id = s.id
        AND sa.is_active = true
        AND sa.alias_name % :query
    )
  )
ORDER BY GREATEST(
  similarity(s.name, :query),
  COALESCE((SELECT MAX(similarity(sa.alias_name, :query))
    FROM supplier_aliases sa
    WHERE sa.supplier_id = s.id AND sa.is_active = true), 0)
) DESC
LIMIT 10;
```

## 3. Resumo de Constraints Técnicas

| Constraint                          | Tipo                        | Descrição                                                                                           |
| ----------------------------------- | --------------------------- | --------------------------------------------------------------------------------------------------- |
| Unicidade temporal de alias         | Trigger                     | Mesmo alias_name (case-insensitive) não pode existir para dois fornecedores com vigência sobreposta |
| is_active exclui de resolução       | Lógica de aplicação + query | Alias inativo ignorado em todas as queries de resolução                                             |
| Auto-alias em renomeação            | Trigger                     | Nome antigo vira alias type='former_name' automaticamente                                           |
| Merge é atômico                     | Edge Function               | Todas as 8 etapas em transação única                                                                |
| Fornecedor absorvido não é deletado | Regra de negócio            | is_active = false, preserva rastro completo                                                         |
| Index trigram para busca fuzzy      | Índice GIN                  | pg_trgm em alias_name e suppliers.name                                                              |

## 4. Consequências

### Positivas

- Um alias resolve para EXATAMENTE um fornecedor em qualquer ponto no tempo
- Histórico preservado: renomeação e merges não perdem dados
- Merge atômico e auditado
- Busca fuzzy performática via trigram

### Negativas

- Triggers adicionam overhead em operações de escrita (aceitável)
- Merge só via Edge Function (não pode ser chamado diretamente do frontend)
- Sem reversão automática de merge no MVP (trade-off por segurança)

## 5. Testes Associados

- **T18 (existente):** CRUD de fornecedor
- **T19 (existente):** Aliases com autocomplete
- **T20 (existente):** Merge de fornecedores
- **Novo: T-ALIAS-01:** Criar dois aliases iguais para fornecedores diferentes no mesmo período → rejeitado
- **Novo: T-ALIAS-02:** Criar dois aliases iguais com períodos NÃO sobrepostos → aceito
- **Novo: T-ALIAS-03:** Alias inativo NÃO aparece em resolução
- **Novo: T-ALIAS-04:** Renomear fornecedor cria alias former_name automaticamente
- **Novo: T-ALIAS-05:** Merge migra todas as referências (transactions, templates, docs, etc.)
- **Novo: T-ALIAS-06:** Merge não deleta fornecedor absorvido (is_active = false)
- **Novo: T-ALIAS-07:** Merge registra audit_log completo
- **Novo: T-ALIAS-08:** Busca fuzzy por trigram retorna fornecedores com nome ou alias similar
- **Novo: T-ALIAS-09:** Resolução temporal: alias expirado NÃO resolve para data posterior

## 6. Etapa de Implementação

**Etapa 1** (Base Estrutural): Tabela, triggers de unicidade e auto-alias, índices trigram
**Etapa 3** (Núcleo Funcional): CRUD de aliases, autocomplete, resolução temporal
**Etapa 5** (Recursos Avançados): Merge atômico via Edge Function, associação retroativa
