/**
 * Validação de CPF pelos dígitos verificadores. Serve só para pegar erro de
 * digitação na hora de guardar o CPF no cofre de segredos — o CPF em si nunca
 * é impresso, logado nem gravado fora do arquivo cifrado.
 */
export function cpfValido(entrada: string): boolean {
  const d = entrada.replace(/\D/g, "");
  if (d.length !== 11 || /^(\d)\1{10}$/.test(d)) return false;
  const verificador = (n: number): number => {
    let soma = 0;
    for (let i = 0; i < n; i++) soma += Number(d[i]) * (n + 1 - i);
    const resto = (soma * 10) % 11;
    return resto === 10 ? 0 : resto;
  };
  return verificador(9) === Number(d[9]) && verificador(10) === Number(d[10]);
}

/** Só os 11 dígitos. */
export function cpfDigitos(entrada: string): string {
  return entrada.replace(/\D/g, "");
}
