## O que muda

<!-- O problema real que isto resolve. Não repita o título. -->

## Definition of Done (§19 do plano mestre)

- [ ] Código implementado
- [ ] Migrations aplicáveis **e reversíveis** (bloco `-- rollback:`)
- [ ] Tipos atualizados (`bun run generate-types`)
- [ ] Testes relevantes passam
- [ ] Logs e erros adequados, sem dados sensíveis
- [ ] Segurança revisada (RLS, `auth.uid()`, escopo por usuário)
- [ ] Documentação atualizada — incl. `docs/specs/00-estado-real.md` se o estado mudou
- [ ] Critérios de aceite demonstrados, com evidência anexada
- [ ] Nenhuma informação sensível commitada
- [ ] Retry/idempotência considerados
- [ ] Impacto nas jornadas registrado

> "Compila" não é Definition of Done.

## Evidência

<!-- Comando executado e saída. Um item só é ✅ com teste que passa E chamador em produção. -->
