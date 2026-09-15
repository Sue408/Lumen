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

const decimal = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 4 });

export const formatMoney = (value: number): string => money.format(value);

export const formatCurrency = (value: number): string => currency.format(value);

export const formatInteger = (value: number): string => integer.format(value);

/** 连续刻度（如纵轴）：最多 4 位小数并去掉尾零，整数部分加千分位。 */
export const formatDecimal = (value: number): string => decimal.format(value);
