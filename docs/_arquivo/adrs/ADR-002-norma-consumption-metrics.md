# ADR-002: Norma de Uso de `consumption_metrics`

**Status:** Aprovado
**Data:** 2026-03-21
**Origem:** Parecer Verônica — Ajuste Obrigatório 4.2
**Ata de referência:** `docs/refinos/2026-03/2026-03-21-13-30-checkpoint-pre-implementacao-ajustes-obrigatorios.md` (Seção 3)

---

## 1. Contexto

A tabela `consumption_metrics` armazena dados de consumo associados a fornecedores (ex: kWh de energia, GB de internet, litros de água). Porém, sem norma formal, ela corre risco de virar "tabela-lixeira" misturando:

- Métricas quantitativas (consumo de energia em kWh)
- Atributos qualitativos (bandeira tarifária verde/amarela)
- Metadados de referência (número do medidor)

A Verônica identificou que essa ambiguidade precisa ser resolvida antes da implementação.

## 2. Decisão

### 2.1. Três Categorias de Dados

| Categoria    | Definição                                                    | Onde registrar                                              |
| ------------ | ------------------------------------------------------------ | ----------------------------------------------------------- |
| **Métrica**  | Valor quantitativo mensurável com unidade de medida          | `consumption_metrics` — campos estruturados                 |
| **Atributo** | Qualificador categórico ou classificatório, sem quantidade   | `consumption_metrics.metadata` com `type: "attribute"`      |
| **Metadado** | Informação de referência estática sobre equipamento/contrato | `supplier_contracts.metadata` ou `supplier_contracts.notes` |

### 2.2. Definição Formal: Métrica

> **Métrica** é um registro quantitativo mensurável associado a um fornecedor em um período de referência, que possui unidade de medida e pode ou não ter valor monetário associado.

**Critérios obrigatórios:**

- `quantity` IS NOT NULL (valor numérico, pode ser 0)
- `metric_name` IS NOT NULL (nome da métrica: "Consumo", "Dados utilizados", etc.)
- `metric_unit` IS NOT NULL (unidade: kWh, GB, m³, minutos, unidades, etc.)

**Campos opcionais:**

- `unit_price` — preço unitário (quando monetizável)
- `subtotal` — valor total da métrica (quantity × unit_price ou valor fixo)

### 2.3. Definição Formal: Atributo Qualitativo

> **Atributo** é uma característica qualitativa associada a um período de referência, que não possui quantidade numérica mensurável.

**Critérios:**

- `quantity` IS NULL
- `metric_name` IS NULL ou descritivo
- `metadata` contém `{"type": "attribute", "attribute_name": "...", "attribute_value": "..."}`
- Preserva série temporal (ex: "março foi bandeira amarela, abril foi verde")

### 2.4. Definição Formal: Metadado

> **Metadado** é uma informação de referência estática sobre o equipamento, instalação ou contrato, que NÃO varia por período.

**Critérios:**

- NÃO vai em `consumption_metrics` (não é periódico)
- Registrar em `supplier_contracts.metadata` ou `supplier_contracts.notes`
- Exemplos: número do medidor, ID de instalação, código do cliente

### 2.5. Regra: Coerência com Valor da Transação

> O somatório dos subtotais das métricas NÃO precisa bater com o valor da transação vinculada.

Motivo: a transação pode incluir impostos, taxas, descontos e arredondamentos que não são decompostos em métricas. As métricas são decomposição PARCIAL e informativa, não contábil.

### 2.6. Constraint no Banco

```sql
ALTER TABLE consumption_metrics ADD CONSTRAINT chk_metric_or_attribute
  CHECK (
    -- Caso 1: métrica quantitativa — precisa de name, unit e quantity
    (quantity IS NOT NULL AND metric_name IS NOT NULL AND metric_unit IS NOT NULL)
    OR
    -- Caso 2: atributo qualitativo — sem quantity, com metadata tipada
    (quantity IS NULL AND metadata IS NOT NULL AND metadata->>'type' = 'attribute')
  );
```

### 2.7. Convenção para o campo `metadata`

#### Atributo qualitativo periódico:

```jsonc
{
  "type": "attribute",
  "attribute_name": "bandeira",
  "attribute_value": "amarela",
}
```

#### Contexto adicional de métrica:

```jsonc
{
  "type": "metric",
  "tariff": "convencional",
  "reading_date": "2026-03-15",
}
```

## 3. Exemplos Válidos

### 3.1. Conta de Energia (Neoenergia) — 3 registros

| metric_name | metric_unit | quantity | unit_price | subtotal | metadata                                                                            |
| ----------- | ----------- | -------- | ---------- | -------- | ----------------------------------------------------------------------------------- |
| Consumo     | kWh         | 320.0000 | 0.875000   | 278.40   | `{"tariff": "convencional"}`                                                        |
| COSIP       | taxa        | 1.0000   | 42.500000  | 42.50    | `{"description": "Contribuição iluminação pública"}`                                |
| NULL        | NULL        | NULL     | NULL       | NULL     | `{"type": "attribute", "attribute_name": "bandeira", "attribute_value": "amarela"}` |

### 3.2. Conta de Internet (Vivo) — 2 registros

| metric_name      | metric_unit | quantity | unit_price | subtotal | metadata                                                                               |
| ---------------- | ----------- | -------- | ---------- | -------- | -------------------------------------------------------------------------------------- |
| Dados utilizados | GB          | 45.0000  | NULL       | NULL     | NULL                                                                                   |
| NULL             | NULL        | NULL     | NULL       | NULL     | `{"type": "attribute", "attribute_name": "plano", "attribute_value": "Fibra 300Mbps"}` |

### 3.3. Assinatura SaaS — GitHub (1 registro)

| metric_name | metric_unit | quantity | unit_price | subtotal | metadata          |
| ----------- | ----------- | -------- | ---------- | -------- | ----------------- |
| Licenças    | unidades    | 1.0000   | 4.000000   | 4.00     | `{"plan": "Pro"}` |

### 3.4. Conta de Água (2 registros)

| metric_name | metric_unit | quantity | unit_price | subtotal | metadata                   |
| ----------- | ----------- | -------- | ---------- | -------- | -------------------------- |
| Consumo     | m³          | 12.0000  | 8.500000   | 102.00   | NULL                       |
| Taxa esgoto | taxa        | 1.0000   | 81.600000  | 81.60    | `{"percent_of_water": 80}` |

## 4. Exemplos Inválidos

| Tentativa                                                                                       | Por que é inválido                                                          | Correção                                                                                              |
| ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `metric_name: "Bandeira"`, `quantity: 1`, `metric_unit: "flag"`                                 | Bandeira não é quantidade mensurável — forçar quantity=1 é semântica errada | Usar metadata: `{"type": "attribute", "attribute_name": "bandeira", "attribute_value": "amarela"}`    |
| `metric_name: "Plano"`, `quantity: 300`, `metric_unit: "Mbps"`                                  | 300Mbps é velocidade contratada, não consumo medido                         | Usar metadata: `{"type": "attribute", "attribute_name": "plano", "attribute_value": "Fibra 300Mbps"}` |
| `metric_name: "Observação"`, `quantity: NULL`, `metadata: {"text": "Medidor trocado em março"}` | Não é métrica nem atributo periódico — é metadado de contrato               | Registrar em `supplier_contracts.notes`                                                               |
| `subtotal: 280.00` sem `quantity` nem `unit_price`                                              | Subtotal sem quantity/unit_price perde rastreabilidade                      | Usar `quantity: 1`, `unit_price: 280.00` se é taxa fixa                                               |
| `metric_name: "Nº Medidor"`, `quantity: 12345`, `metric_unit: "id"`                             | Número do medidor é referência estática, não consumo                        | Registrar em `supplier_contracts.metadata`                                                            |

## 5. Convenções por Tipo de Fornecedor

| Tipo                       | Métricas típicas                | Atributos típicos                | Referências → supplier_contracts |
| -------------------------- | ------------------------------- | -------------------------------- | -------------------------------- |
| **utility** (energia)      | kWh consumo, taxa COSIP         | bandeira, faixa, grupo tarifário | Nº medidor, UC, classe           |
| **utility** (água)         | m³ consumo, taxa esgoto         | faixa de consumo                 | Nº hidrômetro, matrícula         |
| **utility** (gás)          | m³ consumo                      | tarifa residencial/comercial     | Nº medidor                       |
| **telecom**                | GB dados, minutos chamadas      | plano, velocidade contratada     | Nº linha, contrato               |
| **saas**                   | licenças, GB storage, API calls | plano (free/pro/team)            | Subscription ID                  |
| **company** (supermercado) | _geralmente sem métricas_       | _nenhum_                         | _sem contrato_                   |
| **individual** (diarista)  | horas trabalhadas               | _nenhum_                         | _sem contrato formal_            |
| **platform** (iFood)       | pedidos realizados              | _nenhum_                         | _sem contrato_                   |

## 6. Consequências

### Positivas

- Tabela `consumption_metrics` tem semântica clara e auditável
- Constraint no banco impede dados malformados
- Atributos qualitativos preservam série temporal via metadata tipada
- Convenções por tipo de fornecedor guiam implementação

### Negativas

- Requer disciplina do desenvolvedor para classificar corretamente métrica vs atributo
- Atributos qualitativos em JSONB são menos tipados que campos fixos (trade-off aceitável pela flexibilidade)
- Convenção de metadata precisa ser documentada e validada no backend

## 7. Testes Associados

- **T25 (existente):** Registro de métrica de consumo associada a fornecedor
- **T26 (existente):** Consulta de histórico de métricas por fornecedor + período
- **Novo: T-METRIC-01:** Registro de métrica válida (com quantity + unit) → aceito
- **Novo: T-METRIC-02:** Registro de atributo válido (sem quantity, com metadata tipada) → aceito
- **Novo: T-METRIC-03:** Registro sem quantity E sem metadata tipada → rejeitado pelo constraint
- **Novo: T-METRIC-04:** Registro com quantity mas sem metric_unit → rejeitado pelo constraint
- **Novo: T-METRIC-05:** Consulta de série temporal de atributos (bandeira por mês)

## 8. Etapa de Implementação

**Etapa 1** (Base Estrutural): Tabela `consumption_metrics` com constraint `chk_metric_or_attribute`
**Etapa 5** (Recursos Avançados): CRUD de métricas, UI de registro, consultas temporais
