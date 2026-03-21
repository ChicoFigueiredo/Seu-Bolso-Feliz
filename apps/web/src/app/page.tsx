import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

export default function Home() {
  return (
    <div className="container mx-auto flex min-h-screen items-center justify-center p-8">
      <Card className="w-full max-w-lg">
        <CardHeader className="text-center">
          <CardTitle className="text-3xl font-bold">Seu Bolso Feliz</CardTitle>
          <CardDescription>Organização financeira pessoal inteligente</CardDescription>
        </CardHeader>
        <CardContent className="text-center text-muted-foreground">
          <p>MVP em construção. Etapa 0 concluída — monorepo configurado.</p>
        </CardContent>
      </Card>
    </div>
  );
}
