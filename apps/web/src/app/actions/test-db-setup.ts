if (process.env.TEST_DATABASE_URL) {
  process.env.DATABASE_URL = process.env.TEST_DATABASE_URL;
} else {
  // Sem banco de teste: describe.skipIf pula as suítes; apagar DATABASE_URL
  // garante que nenhum teste que escape do skip consiga tocar o banco real.
  delete process.env.DATABASE_URL;
}
