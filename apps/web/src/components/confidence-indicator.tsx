import { cn } from "@/lib/utils";

interface ConfidenceIndicatorProps {
  value: number; // 0-1
  label?: string;
  className?: string;
}

function getConfidenceColor(value: number): string {
  if (value >= 0.8) return "bg-green-500";
  if (value >= 0.6) return "bg-yellow-500";
  if (value >= 0.4) return "bg-orange-500";
  return "bg-red-500";
}

function getConfidenceLabel(value: number): string {
  if (value >= 0.8) return "Alta";
  if (value >= 0.6) return "Média";
  if (value >= 0.4) return "Baixa";
  return "Muito baixa";
}

export function ConfidenceIndicator({ value, label, className }: ConfidenceIndicatorProps) {
  const pct = Math.round(value * 100);
  const color = getConfidenceColor(value);
  const confidenceLabel = label ?? getConfidenceLabel(value);

  return (
    <div className={cn("flex items-center gap-2", className)}>
      <div className="h-2 w-16 overflow-hidden rounded-full bg-muted">
        <div
          className={cn("h-full rounded-full transition-all", color)}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="text-xs text-muted-foreground">
        {pct}% — {confidenceLabel}
      </span>
    </div>
  );
}
