-- La confirmación pertenece a la conversación y se borra al reiniciarla.
ALTER TABLE conversacion_asistente ADD COLUMN lectura_confirmada INTEGER NOT NULL DEFAULT 0;
