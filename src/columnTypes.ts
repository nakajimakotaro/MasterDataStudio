import type { ColumnType, Definition } from "./types.ts";

export const columnTypeLabels: Record<ColumnType, string> = {
  integer: "整数", decimal: "小数", boolean: "真偽値", string: "文字列", date: "日付", time: "時刻", datetime: "日時",
};

/** Compact marker for grid headers; the full label stays in the tooltip. */
export const columnTypeBadges: Record<ColumnType, string> = {
  integer: "#", decimal: "#.", boolean: "T/F", string: "Aa", date: "D", time: "T", datetime: "DT",
};

export function columnType(def: Definition, column: string): ColumnType {
  return def.columnTypes && Object.hasOwn(def.columnTypes, column) ? def.columnTypes[column] : "integer";
}

/** Advisory only: never rewrite or reject the stored string. Empty cells are allowed. */
export function columnTypeError(value: string, type: ColumnType): string | null {
  if (value === "" || type === "string") return null;
  switch (type) {
    case "integer":
      return /^[+-]?\d+$/.test(value) ? null : "整数として解釈できません";
    case "decimal":
      return /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value)
        ? null : "数値として解釈できません";
    case "boolean":
      return value === "1" || value === "0" ? null : "1（真）または 0（偽）を入力してください";
    case "time": {
      if (!/^\d{2}:\d{2}:\d{2}$/.test(value)) return "時刻は HH:mm:ss で入力してください";
      const [hour, minute, second] = value.split(":").map(Number);
      return hour <= 23 && minute <= 59 && second <= 59 ? null : "時刻の範囲が不正です（00:00:00〜23:59:59）";
    }
    case "datetime":
      if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$/.test(value)) return "日時は YYYY-MM-DDTHH:mm:ss で入力してください";
      return columnTypeError(value.slice(0, 10), "date") ?? columnTypeError(value.slice(11), "time");
    case "date": {
      if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return "日付は YYYY-MM-DD で入力してください";
      const [year, month, day] = value.split("-").map(Number);
      const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
      const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
      return year >= 1 && month >= 1 && month <= 12 && day >= 1 && day <= days[month - 1]
        ? null : "存在しない日付です";
    }
  }
}

export type ScriptValue = string | number | boolean;

/** Conversion and validation are independent; NaN/Infinity may flow through scripts. */
export function scriptValue(value: string, type: ColumnType): ScriptValue {
  if (type === "integer" || type === "decimal") return Number(value);
  if (type === "boolean" && (value === "1" || value === "0")) return value === "1";
  return value;
}
