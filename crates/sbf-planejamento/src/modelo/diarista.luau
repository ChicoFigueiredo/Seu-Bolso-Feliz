-- Diarista por dia da semana: diária × quantas vezes o dia da semana cai no mês da
-- competência (ou × a quantidade que o usuário digitou só para aquele mês).
function calcular(entrada, ajuda)
	local ano = tonumber(string.sub(entrada.competencia, 1, 4))
	local mes = tonumber(string.sub(entrada.competencia, 6, 7))
	local diaria = entrada.parametros.diaria
	local dia = entrada.parametros.dia_da_semana
	local n = entrada.quantidade_ajustada
	if n == nil then
		n = ajuda.dias_da_semana_no_mes(ano, mes, dia)
	end
	local nome = ajuda.nome_do_dia_da_semana(dia)
	local plural = n == 1 and "" or "s"
	return {
		valor_centavos = diaria * n,
		quantidade = n,
		explicacao = "diária " .. ajuda.reais(diaria) .. " × " .. n .. " " .. nome .. plural,
	}
end
