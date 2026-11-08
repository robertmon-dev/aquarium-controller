export interface LocalClock {
  month: number;
  minuteOfDay: number;
}

export function localClockFrom(date: Date): LocalClock {
  return {
    month: date.getMonth() + 1,
    minuteOfDay: date.getHours() * 60 + date.getMinutes(),
  };
}
