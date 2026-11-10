import { useEffect, useState } from "react";

export function useTouchControls() {
  const [touch, setTouch] = useState(() => matchMedia("(any-pointer: coarse)").matches);
  useEffect(() => {
    const query = matchMedia("(any-pointer: coarse)");
    const update = () => setTouch(query.matches);
    query.addEventListener("change", update);
    update();
    return () => query.removeEventListener("change", update);
  }, []);
  return touch;
}
