scalable_icon_dir = datadir / 'icons' / 'hicolor' / 'scalable' / 'apps'
install_data(
  'hicolor' / 'scalable' / 'apps' / 'io.github.antonio.Mithrandir.svg',
  install_dir: scalable_icon_dir
)

icon_sizes = ['16x16', '24x24', '32x32', '48x48', '64x64', '128x128', '256x256', '512x512']
foreach size : icon_sizes
  install_data(
    'hicolor' / size / 'apps' / 'io.github.antonio.Mithrandir.png',
    install_dir: datadir / 'icons' / 'hicolor' / size / 'apps'
  )
endforeach

symbolic_icon_dir = datadir / 'icons' / 'hicolor' / 'symbolic' / 'apps'
install_data(
  'hicolor' / 'symbolic' / 'apps' / 'io.github.antonio.Mithrandir-symbolic.svg',
  'hicolor' / 'symbolic' / 'apps' / 'mithrandir-hamburger-symbolic.svg',
  install_dir: symbolic_icon_dir
)

