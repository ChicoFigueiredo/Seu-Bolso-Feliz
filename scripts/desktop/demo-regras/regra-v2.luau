-- v2: favorecido em maiúsculas e a tarifa de 1% do valor, em centavos.
function transformar(entrada, parametros, ajuda)
  return {
    corresponde = true,
    campos = {
      favorecido = string.upper(entrada.campos.favorecido),
      tarifa_centavos = ajuda.mul_div_arred(entrada.campos.valor_centavos, 1, 100),
    },
  }
end
