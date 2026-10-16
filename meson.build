project('io.github.antonio.Mithrandir',
  version: '0.4.0',
  meson_version: '>= 0.62.0',
  default_options: [ 'warning_level=2' ],
)

i18n = import('i18n')
gnome = import('gnome')
python = import('python')

py_installation = python.find_installation('python3')

prefix = get_option('prefix')
bindir = prefix / get_option('bindir')
datadir = prefix / get_option('datadir')
pkgdatadir = datadir / meson.project_name()
pythondir = py_installation.get_install_dir()

conf = configuration_data()
conf.set('APP_ID', meson.project_name())
conf.set('PKGDATADIR', pkgdatadir)
conf.set('VERSION', meson.project_version())
conf.set('localedir', prefix / get_option('localedir'))
conf.set('PYTHON', py_installation.full_path())

subdir('data')
subdir('src')
subdir('po')

gnome.post_install(
  glib_compile_schemas: true,
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
