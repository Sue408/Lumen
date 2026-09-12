const money = new Intl.NumberFormat("zh-CN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

const currency = new Intl.NumberFormat("zh-CN", {
  style: "currency",
  currency: "USD",
  currencyDisplay: "narrowSymbol",
  minimumFractionDigits: 2,
});

const integer = new Intl.NumberFormat("zh-CN");

export const formatMoney = (value: number): string => money.format(value);

export const formatCurrency = (value: number): string => currency.format(value);

export const formatInteger = (value: number): string => integer.format(value);
