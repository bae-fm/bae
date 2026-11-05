import type * as React from 'react';

/** kind: primary is the one action a screen is for, on the right of the footer; secondary is everything else; icon is a glyph-only button (label becomes its accessible name). */
export interface ButtonProps { kind?: 'primary' | 'secondary' | 'icon'; label?: string; off?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function Button(props: ButtonProps): React.ReactElement;

/** A text field with its label on the left (labelWidth px), or the label shown as a placeholder (labelHidden). note is a grey line under the field; error a red one, shown when state is error. row draws it as a row of a Box. */
export interface TextFieldProps { label: string; value?: string; placeholder?: string; state?: 'normal' | 'focused' | 'off' | 'error'; secret?: boolean; note?: string; error?: string; labelHidden?: boolean; labelWidth?: number; row?: boolean }
export declare function TextField(props: TextFieldProps): React.ReactElement;

/** A rounded group of rows; draws a hairline between its children. empty is the sentence shown when it has none. */
export interface BoxProps { elevated?: boolean; empty?: string; children?: React.ReactNode }
export declare function Box(props: BoxProps): React.ReactElement;

/** One line of a Box: label on the left, then value (grey text), any controls passed as children, and a chevron when it opens something. */
export interface RowProps { label: React.ReactNode; note?: string; value?: string; chevron?: boolean; selected?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function Row(props: RowProps): React.ReactElement;

/** One choice of several, as a row of a Box. */
export interface RadioProps { label: string; note?: string; checked?: boolean; onClick?: () => void }
export declare function Radio(props: RadioProps): React.ReactElement;

/** A heading over what follows it, usually one Box. */
export interface SectionProps { title?: string; children?: React.ReactNode }
export declare function Section(props: SectionProps): React.ReactElement;

/** A sheet: title, divider, content. overlay dims the window behind it (the parent must be position: relative). The last child is usually a SheetFooter. */
export interface SheetProps { title?: string; overlay?: boolean; width?: number; children?: React.ReactNode }
export declare function Sheet(props: SheetProps): React.ReactElement;

/** A sheet's buttons, on the right. split puts the first child on the left (New folder…). */
export interface SheetFooterProps { split?: boolean; children?: React.ReactNode }
export declare function SheetFooter(props: SheetFooterProps): React.ReactElement;

/** A ▸ label that shows its children when opened. open sets how it starts. */
export interface DisclosureProps { label?: string; open?: boolean; children?: React.ReactNode }
export declare function Disclosure(props: DisclosureProps): React.ReactElement;

/** A pop-up menu showing its current choice. */
export interface PopUpProps { value?: string; width?: number; off?: boolean; children?: React.ReactNode }
export declare function PopUp(props: PopUpProps): React.ReactElement;

/** A step that waits: its words over a bar. Without value the bar moves; with value (0 to 1) it fills. */
export interface ProgressProps { label?: string; value?: number }
export declare function Progress(props: ProgressProps): React.ReactElement;

/** A line saying what failed and what to do. */
export interface ErrorTextProps { children?: React.ReactNode }
export declare function ErrorText(props: ErrorTextProps): React.ReactElement;

/** A grey line under a control that says what it does or needs. */
export interface NoteProps { children?: React.ReactNode }
export declare function Note(props: NoteProps): React.ReactElement;

export type StatusTone = 'neutral' | 'accent' | 'info' | 'success' | 'warning' | 'danger' | 'activity';
/** A short label on its tone's tint: a status, a count, a tag or a role. */
export interface StatusChipProps { tone?: StatusTone; children?: React.ReactNode }
export declare function StatusChip(props: StatusChipProps): React.ReactElement;

/** A message on its tone's tint. */
export interface NoticeProps { tone?: StatusTone; children?: React.ReactNode }
export declare function Notice(props: NoticeProps): React.ReactElement;

declare global { interface Window { Bae: { Button: typeof Button; TextField: typeof TextField; Box: typeof Box; Row: typeof Row; Radio: typeof Radio; Section: typeof Section; Sheet: typeof Sheet; SheetFooter: typeof SheetFooter; Disclosure: typeof Disclosure; PopUp: typeof PopUp; Progress: typeof Progress; ErrorText: typeof ErrorText; Note: typeof Note; StatusChip: typeof StatusChip; Notice: typeof Notice } } }
