export type SelectValue = string | number | boolean | null;

export interface SelectOption<T extends SelectValue = string> {
  readonly value: T;
  readonly label: string;
  readonly description?: string;
  readonly disabled?: boolean;
}
