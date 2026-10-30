/** Catálogo español: texto fuente en inglés → traducción. */
export const es: Record<string, string> = {
  'Language / Idioma': 'Language / Idioma',
  'Choose the language of the interface. It applies instantly and is saved.':
    'Elige el idioma de la interfaz. Se aplica al instante y se guarda.',
  'Could not save the language': 'No se pudo guardar el idioma',

  // Pantalla de carga (carga.ts, index.html, App.tsx)
  'Loading…': 'Cargando la interfaz…',
  'Loading ARGUS': 'Cargando ARGUS',
  'Reading your settings…': 'Leyendo tu configuración…',
  'Connecting to the bot…': 'Conectando con el bot…',
  'Loading your servers…': 'Cargando tus servidores…',
  'Ready': 'Listo',

  // Secciones
  'Dashboard': 'Panel',
  'Channels': 'Canales',
  'Members': 'Miembros',
  'Roles': 'Roles',
  'Server': 'Servidor',
  'Logs': 'Logs',
  'Settings': 'Config',
  'Sections': 'Secciones',

  // Sidebar
  'Gateway online': 'Gateway en línea',
  'Gateway connecting': 'Gateway conectando',
  'Gateway offline': 'Gateway desconectado',
  'No active bot': 'Sin bot activo',
  'Online': 'En línea',
  'Connecting...': 'Conectando...',
  'Disconnected': 'Desconectado',
  'Bots': 'Bots',
  'Use as the active bot': 'Usar como bot activo',
  'Could not switch bot': 'No se pudo cambiar de bot',
  'Add a bot in Settings to turn it on': 'Añade un bot en Config para encenderlo',
  'Turn off': 'Apagar',
  'Turn on': 'Encender',

  // GuildRail
  "Could not read the bot's servers": 'No se pudieron leer los servidores del bot',
  'Could not open the invite link': 'No se pudo abrir el enlace de invitación',
  'Servers': 'Servidores',
  'Error reading servers: {error}. Press to retry.': 'Error al leer los servidores: {error}. Pulsa para reintentar.',
  '{error} — press to retry': '{error} — pulsa para reintentar',
  'Refresh the server list': 'Actualizar la lista de servidores',
  'Refreshing servers…': 'Actualizando servidores…',
  'Add the bot to a server': 'Añadir el bot a un servidor',

  // Header
  'Gateway live': 'Gateway en vivo',
  'Gateway disconnected': 'Gateway desconectado',
  'Gateway latency': 'Latencia del Gateway',
  'Server members': 'Miembros del servidor',
  'No server selected': 'Ningún servidor seleccionado',
  'No server': 'Sin servidor',

  // Notification
  'Close notification': 'Cerrar notificación',

  // EstadoVista
  'Pick a server in the left column to see its data.': 'Elige un servidor en la columna de la izquierda para ver sus datos.',
  'Loading the server data': 'Cargando los datos del servidor',
  'They are read from Discord when you pick the server. If they do not appear, press Load again (the reason is in Logs).':
    'Se leen de Discord al elegir el servidor. Si no aparecen, pulsa Cargar de nuevo (el motivo queda en Logs).',
  'Load again': 'Cargar de nuevo',

  // components/ui
  'Loading': 'Cargando',
  'Something went wrong': 'Algo salió mal',
  'Retry': 'Reintentar',
  'Delete': 'Borrar',
  'Cancel': 'Cancelar',
  'Type «{nombre}» to confirm': 'Escribe «{nombre}» para confirmar',

  // Dashboard
  'View channels': 'Ver canales',
  'Send messages': 'Enviar mensajes',
  'Read message history': 'Leer el historial de mensajes',
  'Manage channels': 'Gestionar canales',
  'Manage roles': 'Gestionar roles',
  'Moderate members': 'Moderar miembros',
  'Kick members': 'Expulsar miembros',
  'Ban members': 'Banear miembros',
  'Without it the bot cannot see the channels or sync the map.': 'Sin él el bot no ve los canales ni puede sincronizar el mapa.',
  'It will not be able to send messages or replies.': 'No podrá enviar mensajes ni respuestas.',
  'It will not be able to read earlier messages in channels.': 'No podrá leer mensajes anteriores de los canales.',
  'It will not be able to create, edit or delete channels.': 'No podrá crear, editar ni borrar canales.',
  'It will not be able to create, edit or assign roles.': 'No podrá crear, editar ni asignar roles.',
  'It will not be able to time out members.': 'No podrá aplicar timeouts a miembros.',
  'It will not be able to kick members.': 'No podrá expulsar miembros.',
  'It will not be able to ban members.': 'No podrá banear miembros.',
  'Some actions will be disabled.': 'Algunas acciones estarán desactivadas.',
  'Map updated with no structural changes': 'Mapa actualizado sin cambios de estructura',
  'Map resynced': 'Mapa resincronizado',
  'Resync failed: {error}': 'Error al resincronizar: {error}',
  'Register a bot with its token and set it as active to see its servers.': 'Registra un bot con su token y elígelo como activo para ver sus servidores.',
  'Go to Settings': 'Ir a Ajustes',
  'Pick a server in the left column. If none appears, invite the bot from Settings.':
    'Elige un servidor en la columna de la izquierda. Si no aparece ninguno, invita al bot desde Ajustes.',
  'Invite the bot': 'Invitar al bot',
  'Server dashboard': 'Panel del servidor',
  '● Gateway live': '● Gateway en vivo',
  '○ Gateway disconnected': '○ Gateway desconectado',
  'Resyncing…': 'Resincronizando…',
  'Resync': 'Resincronizar',
  '~{count} members (approximate)': '~{count} miembros (aproximado)',
  'Bot permissions': 'Permisos del bot',
  '{count} permissions missing: grant them to the bot role in Discord to enable those actions.':
    'Faltan {count} permisos: concédelos al rol del bot en Discord para activar esas acciones.',
  'Synchronization': 'Sincronización',
  'Step {step}/{total}: {message}': 'Paso {step}/{total}: {message}',
  'Press “Resync” (top right) to repeat the full server synchronization.':
    'Pulsa «Resincronizar» (arriba a la derecha) para repetir la sincronización completa del servidor.',
  'Recent activity': 'Actividad reciente',
  'No map changes yet. Press “Resync” to check whether the server has changed.':
    'Aún no hay cambios en el mapa. Pulsa «Resincronizar» para comprobar si el servidor ha cambiado.',

  // LogsView
  'ERROR': 'ERROR',
  'WARNING': 'AVISO',
  'INFO': 'INFO',
  'SYSTEM': 'SISTEMA',
  'Logs copied': 'Logs copiados',
  'Logs downloaded': 'Logs descargados',
  'Filter by level': 'Filtrar por nivel',
  'All levels': 'Todos los niveles',
  'Search the logs': 'Buscar en los logs',
  'Search the logs…': 'Buscar en los logs…',
  'Auto-scroll': 'Auto-scroll',
  'Copy': 'Copiar',
  'Download': 'Descargar',
  'Clear': 'Limpiar',
  'No logs yet': 'Aún no hay logs',
  'No logs match': 'Ningún log coincide',
  'ARGUS actions will show up here.': 'Las acciones de ARGUS irán apareciendo aquí.',
  'Try another level or another search.': 'Prueba con otro nivel o con otra búsqueda.',
  '{shown} of {total} entries': '{shown} de {total} entradas',
};
