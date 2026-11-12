# ADR-015 — Privacidade: nível de confiança por destinatário, índice local e cifra de disco

**Data:** 2026-11-12 · **Status:** aceita

## Contexto e decisão

Com RAG e análise sobre todo o acervo, muito mais dado pode sair da máquina. Hoje o
texto e a imagem dos documentos vão inteiros para a OpenAI.

**Decisão:**

1. **Nível de confiança.** Cada destinatário tem o seu: um modelo no OpenRouter, um
   cliente MCP, um canal. O nível pode ser mascarado, parcial ou completo, e um
   destinatário novo nasce mascarado. O app registra o que saiu, para onde e a que
   custo.
2. **Índice local.** Embeddings e índice RAG são calculados e guardados na estação. O
   LLM na nuvem recebe só os trechos recuperados, no nível do destinatário.
3. **Cifra de disco.** O dado em repouso na estação fica protegido pela cifra de disco
   do SO (BitLocker, FileVault ou LUKS), e o app avisa quando o disco está em claro. Só
   o chaveiro é cifrado pelo próprio app: keyring do SO, com fallback em arquivo
   protegido por senha-mestra, como o `chess-vault`.
4. **MCP local.** O MCP roda local, por stdio. O remoto fica para depois da fatia de IA.

## Opções descartadas

- **Manter como hoje.** O documento inteiro iria para o provedor.
- **Só modelo local.** É mais lento e menos preciso. O nível de confiança já permite
  manter um destinatário local para o que não pode sair.
- **SQLCipher nos bancos.** Custa de 5 a 15% e exige exceção de licença (`blessing`,
  WTFPL). A compatibilidade com o `sqlite-vec` não foi verificada. E fica incoerente,
  porque os arquivos do cofre continuariam em claro.
- **Cifrar cada arquivo do cofre.** Quebraria as vistas por ciclo, feitas com hardlinks,
  e o acesso direto pelo explorador de arquivos.

## Consequências

- Um arquivo copiado para fora do disco cifrado sai em claro. Esse risco foi aceito.
- O registro do que saiu alimenta os pontos de atenção sobre custo de IA.
