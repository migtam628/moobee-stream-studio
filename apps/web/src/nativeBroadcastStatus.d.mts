export type BroadcastIndicator={label:string;active:boolean;uncertain:boolean;seconds:number|null;kbps:number|null;late:number|null;outputs:number|null}
export function broadcastIndicator(snapshot:unknown,state:'loading'|'available'|'unavailable'):BroadcastIndicator|null
