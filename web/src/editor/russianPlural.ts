/** Число со словом в нужной форме: 1 ошибка, 3 ошибки, 5 ошибок, 21 ошибка, 12 ошибок. */
export function formatRussianCount(count: number, forms: [one: string, few: string, many: string]): string {
  const lastTwoDigits = count % 100;
  const lastDigit = count % 10;
  if (lastTwoDigits >= 11 && lastTwoDigits <= 14) return `${count} ${forms[2]}`;
  if (lastDigit === 1) return `${count} ${forms[0]}`;
  if (lastDigit >= 2 && lastDigit <= 4) return `${count} ${forms[1]}`;
  return `${count} ${forms[2]}`;
}
