-- v1: repete o favorecido como veio no comprovante.
function transformar(entrada, parametros, ajuda)
  return {
    corresponde = true,
    campos = { favorecido = entrada.campos.favorecido },
  }
end
