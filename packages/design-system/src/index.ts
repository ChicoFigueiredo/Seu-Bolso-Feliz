/**
 * @sbf/design-system — a linguagem visual "prosperidade tranquila" do
 * Seu Bolso Feliz.
 *
 * Importe `@sbf/design-system/tokens.css` uma vez no app e envolva a arvore em
 * `<SbfTheme>`. Nenhum componente le cor direta: tudo passa por tokens, então
 * trocar de tema não exige tocar em componente.
 */

export { SbfTheme } from "./theme";
export type { SbfThemeProps } from "./theme";

export {
  SBF_THEMES,
  SBF_DARK_THEMES,
  toneStyles,
  baseType,
  tabular,
  formatBRL,
  formatDate,
} from "./tokens";
export type { SbfThemeName, SbfTone } from "./tokens";

/* Primitivas */
export { Avatar } from "./primitives/Avatar";
export type { AvatarProps } from "./primitives/Avatar";
export { Button } from "./primitives/Button";
export type { ButtonProps, ButtonVariant, ButtonSize } from "./primitives/Button";
export { Card } from "./primitives/Card";
export type { CardProps } from "./primitives/Card";
export { ChipGroup } from "./primitives/ChipGroup";
export type { ChipGroupProps, ChipOption } from "./primitives/ChipGroup";
export { EmptyState } from "./primitives/EmptyState";
export type { EmptyStateProps } from "./primitives/EmptyState";
export { Field } from "./primitives/Field";
export type { FieldProps } from "./primitives/Field";
export { Icon } from "./primitives/Icon";
export type { IconProps } from "./primitives/Icon";
export { MetricLabel } from "./primitives/MetricLabel";
export type { MetricLabelProps } from "./primitives/MetricLabel";
export { MetricValue } from "./primitives/MetricValue";
export type { MetricValueProps } from "./primitives/MetricValue";
export { Pill } from "./primitives/Pill";
export type { PillProps } from "./primitives/Pill";
export { ProgressBar } from "./primitives/ProgressBar";
export type { ProgressBarProps } from "./primitives/ProgressBar";
export { SectionTitle } from "./primitives/SectionTitle";
export type { SectionTitleProps } from "./primitives/SectionTitle";
export { SegmentedBar } from "./primitives/SegmentedBar";
export type { SegmentedBarProps, SegmentedBarSegment } from "./primitives/SegmentedBar";
export { Sheet } from "./primitives/Sheet";
export type { SheetProps } from "./primitives/Sheet";
export { Toast } from "./primitives/Toast";
export type { ToastProps } from "./primitives/Toast";

/* Financeiro */
export { AIReviewItem } from "./financial/AIReviewItem";
export type { AIReviewItemProps } from "./financial/AIReviewItem";
export { AccountBalanceCard } from "./financial/AccountBalanceCard";
export type { AccountBalanceCardProps } from "./financial/AccountBalanceCard";
export { AllocationBar } from "./financial/AllocationBar";
export type { AllocationBarProps, Allocation } from "./financial/AllocationBar";
export { ConnectionStatusCard } from "./financial/ConnectionStatusCard";
export type { ConnectionStatusCardProps } from "./financial/ConnectionStatusCard";
export { CreditCardSummary } from "./financial/CreditCardSummary";
export type { CreditCardSummaryProps } from "./financial/CreditCardSummary";
export { DebtProgressCard } from "./financial/DebtProgressCard";
export type { DebtProgressCardProps } from "./financial/DebtProgressCard";
export { DocumentInboxItem } from "./financial/DocumentInboxItem";
export type { DocumentInboxItemProps, DocumentState } from "./financial/DocumentInboxItem";
export { FinancialCenterCard } from "./financial/FinancialCenterCard";
export type { FinancialCenterCardProps } from "./financial/FinancialCenterCard";
export { FinancialTimeline } from "./financial/FinancialTimeline";
export type { FinancialTimelineProps } from "./financial/FinancialTimeline";
export { GoalProgress } from "./financial/GoalProgress";
export type { GoalProgressProps } from "./financial/GoalProgress";
export { MoneyMetricCard } from "./financial/MoneyMetricCard";
export type { MoneyMetricCardProps } from "./financial/MoneyMetricCard";
export { PendingBox } from "./financial/PendingBox";
export type { PendingBoxProps, PendingItem } from "./financial/PendingBox";
export { PeriodSummary } from "./financial/PeriodSummary";
export type { PeriodSummaryProps } from "./financial/PeriodSummary";
export { RecurringItem } from "./financial/RecurringItem";
export type { RecurringItemProps } from "./financial/RecurringItem";
export { TransactionRow } from "./financial/TransactionRow";
export type { TransactionRowProps } from "./financial/TransactionRow";
export { UpcomingCommitment } from "./financial/UpcomingCommitment";
export type { UpcomingCommitmentProps } from "./financial/UpcomingCommitment";

/* Navegação */
export { AppSidebar } from "./chrome/AppSidebar";
export type { AppSidebarProps, SidebarNavItem } from "./chrome/AppSidebar";
export { BottomNav } from "./chrome/BottomNav";
export type { BottomNavProps, BottomNavTab } from "./chrome/BottomNav";
export { MobileHeader } from "./chrome/MobileHeader";
export type { MobileHeaderProps } from "./chrome/MobileHeader";
export { TopBar } from "./chrome/TopBar";
export type { TopBarProps } from "./chrome/TopBar";
