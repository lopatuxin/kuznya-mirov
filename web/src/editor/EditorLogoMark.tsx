type EditorLogoMarkProps = { size?: number };

/** Знак редактора — наковальня с искрой над ней. */
export function EditorLogoMark({ size = 28 }: EditorLogoMarkProps): React.JSX.Element {
  return (
    <svg className="editor-logo-mark" width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
      <rect width="32" height="32" rx="8" className="editor-logo-mark__tile" />
      <path
        className="editor-logo-mark__anvil"
        d="M5 13h16c2.8 0 5 1.2 6 3h-6.3c-.8 0-1.4.6-1.4 1.4v1.7c0 .8.6 1.4 1.4 1.4h.6V24h-9v-3.5h.6c.8 0 1.4-.6 1.4-1.4v-1.7c0-.8-.6-1.4-1.4-1.4H8.4C6.5 16 5 14.8 5 13Z"
      />
      <path className="editor-logo-mark__spark" d="M16.5 5.5v3.2M12.4 7.2l1.6 2M20.6 7.2l-1.6 2" />
    </svg>
  );
}
